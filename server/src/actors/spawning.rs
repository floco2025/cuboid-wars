use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};
use std::f32::consts::TAU;

use crate::{
    actors::{
        ActorCharacter, ActorCrushed, ActorInfo, ActorMap, ActorRespawnState, ActorRespawnTimers, ActorSpawner,
        PendingActorSpawn, PendingActorSpawns,
    },
    characters::generate_actor_spawn_position_in_zone,
    config::{ActorRespawnScope, ServerGameplayConfig},
    map::{ActorSpawnZone, MapConfig},
};
use common::{
    config::{ActorGameplayConfig, ActorMovementConfig},
    map::Carriers,
    physics::{CharacterSupport, CharacterVerticalVelocity, CollisionWorld},
    protocol::{
        ActorAnchor, ActorMarker, ActorMoveIntent, FaceYaw, Health, MapSettings, PlateState, PlayerMarker, Position,
        ServerTick, sequence_is_newer,
    },
};

pub fn actor_respawns_active(actors: Res<ActorMap>, timers: Res<ActorRespawnTimers>) -> bool {
    actors.has_vacated_spawn_zones() || !timers.0.is_empty()
}

pub fn pending_actor_spawns_active(pending: Res<PendingActorSpawns>) -> bool {
    !pending.0.is_empty()
}

pub(crate) fn reset_actors(
    commands: &mut Commands,
    actors: &mut ActorMap,
    pending: &mut PendingActorSpawns,
    timers: &mut ActorRespawnTimers,
    map_config: &MapConfig,
    scope: ActorRespawnScope,
) {
    if scope == ActorRespawnScope::All {
        for info in actors.values() {
            commands.entity(info.entity).despawn();
        }
        actors.clear();
        pending.0.clear();
    }
    timers.0.clear();
    timers
        .0
        .extend((0..map_config.actor_spawn_zones.len()).map(|zone_idx| (zone_idx, ActorRespawnState::Reset)));
}

fn arm_actor_respawn(timers: &mut ActorRespawnTimers, zone_idx: usize, respawn_secs: f32) {
    timers
        .0
        .entry(zone_idx)
        .or_insert(ActorRespawnState::Cooldown(respawn_secs));
}

fn tick_actor_respawns(timers: &mut ActorRespawnTimers, delta: f32) -> Vec<usize> {
    timers
        .0
        .iter_mut()
        .filter_map(|(zone_idx, state)| match state {
            ActorRespawnState::Cooldown(remaining_secs) => {
                *remaining_secs -= delta;
                (*remaining_secs <= 0.0).then_some(*zone_idx)
            }
            ActorRespawnState::Reset | ActorRespawnState::WaitingForSpace => Some(*zone_idx),
            ActorRespawnState::Inactive => None,
        })
        .collect()
}

// A switched zone follows its switch: off parks it `Inactive`, dropping
// any countdown, wait, or reset; turning on starts its kind's countdown,
// after which every vacancy fills. An active zone is otherwise left to the
// ordinary rules. Runs before this tick's vacancies and countdowns.
fn sync_switched_zones(
    timers: &mut ActorRespawnTimers,
    map_config: &MapConfig,
    plates: &PlateState,
    config: &ServerGameplayConfig,
) {
    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        let Some(switch) = zone.switch else {
            continue;
        };
        if plates.is_active(switch) == zone.switch_inverted {
            timers.0.insert(zone_idx, ActorRespawnState::Inactive);
        } else if timers.0.get(&zone_idx) == Some(&ActorRespawnState::Inactive) {
            let respawn_secs = config
                .expect_actor(&zone.kind)
                .respawn_secs
                .expect("switched zone's kind has no respawn time after validation");
            timers.0.insert(zone_idx, ActorRespawnState::Cooldown(respawn_secs));
        }
    }
}

// Startup-only: fill every spawn zone to its `count`, irrespective of
// `respawn_secs` — initial fill is universal — except a switched zone whose
// condition does not hold yet, which waits for its switch like
// `sync_switched_zones`. Spawns are queued, not spawned: each waits out its
// beam-in warning window in `PendingActorSpawns` before
// `actors_pending_spawn_system` materializes it.
pub fn actors_initial_spawn_system(
    mut pending: ResMut<PendingActorSpawns>,
    mut spawner: ResMut<ActorSpawner>,
    mut timers: ResMut<ActorRespawnTimers>,
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    plates: Res<PlateState>,
    tick: Res<ServerTick>,
    players: Query<&Position, With<PlayerMarker>>,
) {
    let mut planner = SpawnPlanner {
        pending: &mut pending,
        spawner: &mut spawner,
        timers: &mut timers,
        occupied_positions: players.iter().copied().collect(),
        rng: rng(),
        map_config: &map_config,
        carriers: &carriers,
        collision_world: &collision_world,
        config: &server_gameplay_config,
        tick: tick.0,
    };
    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        if zone
            .switch
            .is_some_and(|switch| plates.is_active(switch) == zone.switch_inverted)
        {
            planner.timers.0.insert(zone_idx, ActorRespawnState::Inactive);
        } else {
            planner.queue_zone(zone_idx, zone, zone.count);
        }
    }
}

pub fn actors_respawn_system(
    mut pending: ResMut<PendingActorSpawns>,
    mut spawner: ResMut<ActorSpawner>,
    mut timers: ResMut<ActorRespawnTimers>,
    mut actors: ResMut<ActorMap>,
    time: Res<Time>,
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    plates: Res<PlateState>,
    tick: Res<ServerTick>,
    players: Query<&Position, With<PlayerMarker>>,
    actor_positions: Query<&Position, (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    sync_switched_zones(&mut timers, &map_config, &plates, &server_gameplay_config);
    // A zone gets one timer for all vacancies; later deaths do not restart it
    // (and an `Inactive` entry stays: a kill while the switch is off arms nothing).
    let dt = time.delta_secs();
    for zone_idx in actors.drain_vacated_spawn_zones() {
        let Some(zone) = map_config.actor_spawn_zones.get(zone_idx) else {
            continue;
        };
        let Some(respawn_secs) = server_gameplay_config.expect_actor(&zone.kind).respawn_secs else {
            continue;
        };
        arm_actor_respawn(&mut timers, zone_idx, respawn_secs);
    }

    let due_zones = tick_actor_respawns(&mut timers, dt);
    if due_zones.is_empty() {
        return;
    }

    let mut live_by_zone = vec![0u32; map_config.actor_spawn_zones.len()];
    for info in actors.values() {
        if let Some(count) = live_by_zone.get_mut(info.spawn_zone_index) {
            *count += 1;
        }
    }
    // Pending spawns already count toward quota and reserve their positions.
    let mut occupied_positions: Vec<Position> = players.iter().chain(&actor_positions).copied().collect();
    for entry in &pending.0 {
        if let Some(count) = live_by_zone.get_mut(entry.zone_idx) {
            *count += 1;
        }
        occupied_positions.push(entry.world_position(&carriers));
    }
    let mut planner = SpawnPlanner {
        pending: &mut pending,
        spawner: &mut spawner,
        timers: &mut timers,
        occupied_positions,
        rng: rng(),
        map_config: &map_config,
        carriers: &carriers,
        collision_world: &collision_world,
        config: &server_gameplay_config,
        tick: tick.0,
    };
    for zone_idx in due_zones {
        let Some(zone) = map_config.actor_spawn_zones.get(zone_idx) else {
            planner.timers.0.remove(&zone_idx);
            continue;
        };
        let missing = zone.count.saturating_sub(live_by_zone[zone_idx]);
        planner.queue_zone(zone_idx, zone, missing);
    }
}

// Queues beam-ins for zones' vacancies within one system run, reserving an
// id, spot, and heading for each, and records why a zone could not be filled.
struct SpawnPlanner<'a> {
    pending: &'a mut PendingActorSpawns,
    spawner: &'a mut ActorSpawner,
    timers: &'a mut ActorRespawnTimers,
    // Every spot already taken: players, live actors, and the spawns reserved so far.
    occupied_positions: Vec<Position>,
    rng: ThreadRng,
    map_config: &'a MapConfig,
    carriers: &'a Carriers,
    collision_world: &'a CollisionWorld,
    config: &'a ServerGameplayConfig,
    tick: u32,
}

impl SpawnPlanner<'_> {
    fn queue_zone(&mut self, zone_idx: usize, zone: &ActorSpawnZone, missing: u32) {
        let config = self.config;
        let kind_config = config.expect_actor(&zone.kind);
        for _ in 0..missing {
            if !self.queue_one(zone_idx, zone, &kind_config.character) {
                let previous = self.timers.0.get(&zone_idx).copied();
                let retry_immediately = matches!(
                    previous,
                    Some(ActorRespawnState::Reset | ActorRespawnState::WaitingForSpace)
                );
                match kind_config.respawn_secs {
                    Some(respawn_secs) if !kind_config.character.immovable && !retry_immediately => {
                        warn!(
                            "actor spawn zone {zone_idx} on carrier {} has no clear spot for a {:?}; retrying after its respawn time",
                            zone.carrier.0, zone.kind
                        );
                        self.timers
                            .0
                            .insert(zone_idx, ActorRespawnState::Cooldown(respawn_secs));
                    }
                    _ => {
                        self.timers.0.insert(zone_idx, ActorRespawnState::WaitingForSpace);
                        if previous != Some(ActorRespawnState::WaitingForSpace) {
                            warn!(
                                "actor spawn zone {zone_idx} on carrier {} has no clear spot for a {:?}; waiting for space",
                                zone.carrier.0, zone.kind
                            );
                        }
                    }
                }
                return;
            }
        }
        self.timers.0.remove(&zone_idx);
    }

    // Reserve an id, spot, and heading for one actor and queue it for beam-in.
    // The heading is rolled now so the client ghost and the materialized actor
    // face the same way. The spot is kept in the zone's carrier frame, so it
    // rides the carrier through the warning window, which runs from `tick`.
    // False when the zone has no clear spot.
    fn queue_one(&mut self, zone_idx: usize, zone: &ActorSpawnZone, actor_config: &ActorGameplayConfig) -> bool {
        let Some(pos) = generate_actor_spawn_position_in_zone(
            self.map_config,
            self.carriers,
            zone,
            self.collision_world,
            &self.occupied_positions,
            actor_config,
        ) else {
            return false;
        };
        self.occupied_positions.push(pos);

        self.pending.0.push(PendingActorSpawn {
            actor_id: self.spawner.allocate(),
            zone_idx,
            kind: zone.kind.clone(),
            carrier: zone.carrier,
            pos: self.carriers.pose(zone.carrier).inverse_transform_position(&pos),
            face_yaw: self.rng.random_range(0.0..TAU),
            reserved_tick: self.tick,
            due_tick: self.tick.wrapping_add(
                self.config
                    .actors
                    .settings
                    .spawn_warning_ticks(self.config.network.server_hz),
            ),
        });
        true
    }
}

pub(crate) fn expedite_actor_respawns(
    actors: &ActorMap,
    pending: &mut PendingActorSpawns,
    timers: &mut ActorRespawnTimers,
    map_config: &MapConfig,
    server_gameplay_config: &ServerGameplayConfig,
    tick: u32,
    actor_kind: Option<&str>,
) -> usize {
    let mut occupied_by_zone = vec![0u32; map_config.actor_spawn_zones.len()];
    for info in actors.values() {
        if let Some(count) = occupied_by_zone.get_mut(info.spawn_zone_index) {
            *count += 1;
        }
    }
    let mut respawning = 0usize;
    for spawn in &mut pending.0 {
        if let Some(count) = occupied_by_zone.get_mut(spawn.zone_idx) {
            *count += 1;
        }
        if actor_kind.is_none_or(|kind| spawn.kind == kind) {
            spawn.due_tick = tick;
            respawning += 1;
        }
    }

    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        if actor_kind.is_some_and(|kind| zone.kind != kind) {
            continue;
        }
        let kind_server_config = server_gameplay_config.expect_actor(&zone.kind);
        if kind_server_config.respawn_secs.is_some() {
            let missing = zone.count.saturating_sub(occupied_by_zone[zone_idx]);
            if missing > 0 {
                let state = timers.0.entry(zone_idx).or_insert(ActorRespawnState::Cooldown(0.0));
                match state {
                    ActorRespawnState::Cooldown(remaining_secs) => *remaining_secs = 0.0,
                    // A switched-off zone stays empty; its switch decides.
                    ActorRespawnState::Inactive => continue,
                    ActorRespawnState::Reset | ActorRespawnState::WaitingForSpace => {}
                }
                respawning += missing as usize;
            }
        }
    }

    respawning
}

// Materializes the spawns due by this tick at their reserved spot,
// unconditionally — a player squatting on it resolves via contact
// detonation on the next tick. Runs in `Prepare`, so a pending entry's
// removal and its actor's appearance land in the same snapshot.
pub fn actors_pending_spawn_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    mut pending: ResMut<PendingActorSpawns>,
    tick: Res<ServerTick>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    map_settings: Res<MapSettings>,
    carriers: Res<Carriers>,
) {
    let due = take_due_spawns(&mut pending.0, tick.0);
    if due.is_empty() {
        return;
    }
    for spawn in due {
        let max_health = server_gameplay_config.combat.health.expect_actor(&spawn.kind).max;
        let character = &server_gameplay_config.expect_actor(&spawn.kind).character;
        let movement = (!character.immovable).then(|| *map_settings.movement.expect_actor(&spawn.kind));
        materialize_actor(
            &mut commands,
            &mut actors,
            &carriers,
            max_health,
            character,
            movement,
            spawn,
        );
    }
}

fn take_due_spawns(pending: &mut Vec<PendingActorSpawn>, tick: u32) -> Vec<PendingActorSpawn> {
    let (due, rest): (Vec<_>, Vec<_>) = pending
        .drain(..)
        .partition(|entry| !sequence_is_newer(entry.due_tick, tick));
    *pending = rest;
    due
}

fn materialize_actor(
    commands: &mut Commands,
    actors: &mut ActorMap,
    carriers: &Carriers,
    max_health: f32,
    character: &ActorGameplayConfig,
    movement: Option<ActorMovementConfig>,
    spawn: PendingActorSpawn,
) {
    let move_intent = ActorMoveIntent::Idle;
    let entity = commands
        .spawn((
            ActorMarker,
            spawn.actor_id,
            spawn.world_position(carriers),
            move_intent,
            FaceYaw(spawn.face_yaw),
            CharacterVerticalVelocity::default(),
            CharacterSupport::Airborne,
            Health(max_health),
            ActorCrushed::default(),
            ActorCharacter(character.clone()),
        ))
        .id();

    if let Some(movement) = movement {
        commands.entity(entity).insert(movement);
    }
    let mut info = ActorInfo::new(entity, spawn.zone_idx, spawn.kind, spawn.carrier);
    info.anchor = movement.is_none().then_some(ActorAnchor {
        carrier: spawn.carrier,
        pos: spawn.pos,
    });
    actors.insert(spawn.actor_id, info);
}

#[cfg(test)]
#[path = "tests/spawning.rs"]
mod tests;
