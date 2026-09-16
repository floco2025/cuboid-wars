use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};
use std::f32::consts::TAU;

use crate::{
    actors::{
        ActorCharacter, ActorCrushed, ActorInfo, ActorLanding, ActorMap, ActorSpawner, PendingActorSpawn,
        PendingActorSpawns,
    },
    characters::{generate_flying_spawn_position, generate_ground_actor_spawn_position},
    config::{ActorRespawnScope, ServerGameplayConfig},
    map::{ActorSpawnZone, MapConfig},
    players::PlayerMap,
};
use common::{
    config::{ActorGameplayConfig, ActorMovementConfig, CharacterPhysicsConfig},
    map::Carriers,
    physics::{CharacterSupport, CharacterVerticalVelocity, CollisionWorld, character_positions_intersect},
    protocol::{
        ActorAnchor, ActorMarker, ActorMoveIntent, BarrierId, FaceYaw, Health, MapSettings, PlateState, PlayerMarker,
        Position, ServerTick, sequence_is_newer,
    },
};

pub fn pending_actor_spawns_active(pending: Res<PendingActorSpawns>) -> bool {
    !pending.0.is_empty()
}

pub(crate) fn reset_actors(
    commands: &mut Commands,
    actors: &mut ActorMap,
    pending: &mut PendingActorSpawns,
    spawner: &mut ActorSpawner,
    scope: ActorRespawnScope,
) {
    if scope == ActorRespawnScope::All {
        for info in actors.values() {
            commands.entity(info.entity).despawn();
        }
        actors.clear();
        pending.0.clear();
    }
    // A reset forgives every kill so far, including those not yet drained.
    actors.forget_vacated_spawn_zones();
    spawner.refills.clear();
}

// Each vacated slot waits its own `respawn_secs` and then rejoins the zone's
// deficit: the target for the logged-in players less its live, announced, and
// waiting slots. The deficit fills every tick the switch allows, so the first
// fill, joins, resets, and expired countdowns all spawn the same way.
pub fn actors_respawn_system(
    mut pending: ResMut<PendingActorSpawns>,
    mut spawner: ResMut<ActorSpawner>,
    mut actors: ResMut<ActorMap>,
    time: Res<Time>,
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    plates: Res<PlateState>,
    tick: Res<ServerTick>,
    players: Query<&Position, With<PlayerMarker>>,
    actor_positions: Query<(&Position, &ActorCharacter), (With<ActorMarker>, Without<PlayerMarker>)>,
    player_map: Res<PlayerMap>,
) {
    let player_count = player_map.logged_in_count();
    for zone_idx in actors.drain_vacated_spawn_zones() {
        if let Some(zone) = map_config.actor_spawn_zones.get(zone_idx) {
            spawner.refills.entry(zone_idx).or_default().push(zone.respawn_secs);
        }
    }
    let delta = time.delta_secs();
    for refills in spawner.refills.values_mut() {
        for secs in refills.iter_mut().flatten() {
            *secs -= delta;
        }
        refills.retain(|secs| secs.is_none_or(|secs| secs > 0.0));
    }

    let mut occupied_by_zone = vec![0u32; map_config.actor_spawn_zones.len()];
    for info in actors.values() {
        if let Some(count) = occupied_by_zone.get_mut(info.spawn_zone_index) {
            *count += 1;
        }
    }
    // Pending spawns already count toward quota and reserve their positions.
    let mut occupied_positions: Vec<_> = players
        .iter()
        .map(|p| (*p, server_gameplay_config.player.gameplay.physics()))
        .chain(actor_positions.iter().map(|(p, c)| (*p, c.0.physics())))
        .collect();
    for entry in &pending.0 {
        if let Some(count) = occupied_by_zone.get_mut(entry.zone_idx) {
            *count += 1;
        }
        occupied_positions.push((
            entry.world_position(&carriers),
            server_gameplay_config.expect_actor(&entry.kind).character.physics(),
        ));
    }
    let mut planner = SpawnPlanner {
        pending: &mut pending,
        spawner: &mut spawner,
        occupied_positions,
        rng: rng(),
        map_config: &map_config,
        carriers: &carriers,
        collision_world: &collision_world,
        config: &server_gameplay_config,
        tick: tick.0,
        open: &plates.open_barriers,
    };
    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        // A zone its switch holds back keeps its deficit and fills the tick the switch allows.
        if !zone.is_enabled(&plates) {
            continue;
        }
        let waiting = planner
            .spawner
            .refills
            .get(&zone_idx)
            .map_or(0, |refills| refills.len() as u32);
        let missing = zone
            .target_count(player_count)
            .saturating_sub(occupied_by_zone[zone_idx] + waiting);
        planner.queue_zone(zone_idx, zone, missing);
    }
}

// Queues beam-ins for zones' deficits within one system run, reserving an
// id, spot, and heading for each.
struct SpawnPlanner<'a> {
    pending: &'a mut PendingActorSpawns,
    spawner: &'a mut ActorSpawner,
    // Every spot already taken: players, live actors, and the spawns reserved so far.
    occupied_positions: Vec<(Position, CharacterPhysicsConfig)>,
    open: &'a [BarrierId],
    rng: ThreadRng,
    map_config: &'a MapConfig,
    carriers: &'a Carriers,
    collision_world: &'a CollisionWorld,
    config: &'a ServerGameplayConfig,
    tick: u32,
}

impl SpawnPlanner<'_> {
    // Queues `missing` beam-ins. A zone with no clear spot keeps its deficit for
    // the next tick and warns once, again after a later spawn has succeeded.
    fn queue_zone(&mut self, zone_idx: usize, zone: &ActorSpawnZone, missing: u32) {
        let config = self.config;
        let character = &config.expect_actor(&zone.kind).character;
        for _ in 0..missing {
            if !self.queue_one(zone_idx, zone, character) {
                if self.spawner.blocked.insert(zone_idx) {
                    warn!(
                        "actor spawn zone {zone_idx} on carrier {} has no clear spot for a {:?}; retrying every tick",
                        zone.carrier.0, zone.kind
                    );
                }
                return;
            }
            self.spawner.blocked.remove(&zone_idx);
        }
    }

    // Reserve an id, spot, and heading for one actor and queue it for beam-in.
    // The heading is rolled now so the client ghost and the materialized actor
    // face the same way. The spot is kept in the zone's carrier frame, so it
    // rides the carrier through the warning window, which runs from `tick`.
    // False when the zone has no clear spot.
    fn queue_one(&mut self, zone_idx: usize, zone: &ActorSpawnZone, actor_config: &ActorGameplayConfig) -> bool {
        let pos = if actor_config.flies() {
            generate_flying_spawn_position(
                self.map_config.grid(zone.carrier),
                self.carriers,
                zone,
                self.collision_world,
                &self.occupied_positions,
                actor_config.physics(),
                self.open,
            )
        } else {
            generate_ground_actor_spawn_position(
                self.map_config,
                self.carriers,
                zone,
                self.collision_world,
                &self.occupied_positions.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
                actor_config,
            )
        };
        let Some(pos) = pos else {
            return false;
        };
        self.occupied_positions.push((pos, actor_config.physics()));

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

// `/respawn`: every announced beam-in of the kind is due now, and every slot
// still counting down in a zone that refills is due next tick.
pub(crate) fn expedite_actor_respawns(
    actors: &ActorMap,
    pending: &mut PendingActorSpawns,
    spawner: &mut ActorSpawner,
    map_config: &MapConfig,
    player_count: usize,
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
        if actor_kind.is_some_and(|kind| zone.kind != kind) || zone.respawn_secs.is_none() {
            continue;
        }
        let missing = zone
            .target_count(player_count)
            .saturating_sub(occupied_by_zone[zone_idx]);
        if missing == 0 {
            continue;
        }
        if let Some(refills) = spawner.refills.get_mut(&zone_idx) {
            for secs in refills.iter_mut().flatten() {
                *secs = 0.0;
            }
        }
        respawning += missing as usize;
    }

    respawning
}

// Prepare keeps a warning's removal and its actor's appearance in the same snapshot.
pub fn actors_pending_spawn_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    mut pending: ResMut<PendingActorSpawns>,
    tick: Res<ServerTick>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    map_settings: Res<MapSettings>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    plates: Res<PlateState>,
    bodies: Query<(&Position, Option<&ActorCharacter>), Or<(With<PlayerMarker>, With<ActorMarker>)>>,
) {
    let due = take_due_spawns(&mut pending.0, tick.0);
    if due.is_empty() {
        return;
    }
    let mut occupied: Vec<_> = bodies
        .iter()
        .map(|(pos, actor)| {
            (
                *pos,
                actor.map_or_else(|| server_gameplay_config.player.gameplay.physics(), |c| c.0.physics()),
            )
        })
        .collect();
    for spawn in due {
        let max_health = server_gameplay_config.combat.health.expect_actor(&spawn.kind).max;
        let character = &server_gameplay_config.expect_actor(&spawn.kind).character;
        let pos = spawn.world_position(&carriers);
        if character.flies()
            && (collision_world.character_overlaps_solid(&pos, character.physics(), &plates.open_barriers)
                || occupied
                    .iter()
                    .any(|(other, physics)| character_positions_intersect(&pos, character.physics(), other, *physics)))
        {
            // Dropped from the quota, so the respawn system reserves a fresh
            // spot this tick.
            continue;
        }
        occupied.push((pos, character.physics()));
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
            ActorLanding::default(),
            ActorCharacter(character.clone()),
        ))
        .id();

    if let Some(movement) = movement {
        commands.entity(entity).insert(movement);
    }
    let mut info = ActorInfo::new(entity, spawn.zone_idx, spawn.kind, spawn.carrier);
    info.flight = character.flies().then(Default::default);
    info.anchor = movement.is_none().then_some(ActorAnchor {
        carrier: spawn.carrier,
        pos: spawn.pos,
    });
    actors.insert(spawn.actor_id, info);
}

#[cfg(test)]
#[path = "tests/spawning.rs"]
mod tests;
