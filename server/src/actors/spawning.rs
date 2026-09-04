use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};
use std::f32::consts::TAU;

use crate::{
    actors::{ActorInfo, ActorMap, ActorRespawnTimers, ActorSpawner, PendingActorSpawn, PendingActorSpawns},
    characters::generate_actor_spawn_position_in_zone,
    config::ServerGameplayConfig,
    map::MapConfig,
};
use common::{
    config::{ActorMovementConfig, GameplayConfig},
    map::MapGeometry,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorMarker, ActorMoveIntent, FaceYaw, Health, MapSettings, PlayerMarker, Position},
};

pub fn actor_respawns_active(actors: Res<ActorMap>, timers: Res<ActorRespawnTimers>) -> bool {
    actors.has_vacated_spawn_zones() || !timers.0.is_empty()
}

pub fn pending_actor_spawns_active(pending: Res<PendingActorSpawns>) -> bool {
    !pending.0.is_empty()
}

fn arm_actor_respawn(timers: &mut ActorRespawnTimers, zone_idx: usize, respawn_secs: f32) {
    timers.0.entry(zone_idx).or_insert(respawn_secs);
}

fn tick_actor_respawns(timers: &mut ActorRespawnTimers, delta: f32) -> Vec<usize> {
    for remaining_secs in timers.0.values_mut() {
        *remaining_secs -= delta;
    }
    timers
        .0
        .iter()
        .filter_map(|(zone_idx, remaining_secs)| (*remaining_secs <= 0.0).then_some(*zone_idx))
        .collect()
}

// Startup-only: fill every spawn zone to its `count`. Runs once when the
// world boots, irrespective of `respawns` — initial fill is universal.
// Spawns are queued, not spawned: each waits out its beam-in warning window
// in `PendingActorSpawns` before `actors_pending_spawn_system` materializes it.
pub fn actors_initial_spawn_system(
    mut pending: ResMut<PendingActorSpawns>,
    mut spawner: ResMut<ActorSpawner>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    players: Query<&Position, With<PlayerMarker>>,
) {
    // Avoid spawning on top of any players that may already exist (none, in
    // practice, at Startup — but cheap and consistent with the respawn path).
    let mut occupied_positions: Vec<Position> = players.iter().copied().collect();
    let mut rng = rng();

    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        // Configs are cross-validated against the map at startup, so any
        // zone kind here is guaranteed to resolve in both configs.
        let actor_config = gameplay_config.expect_actor(&zone.kind);
        let actor_physics = actor_config.physics();
        for _ in 0..zone.count {
            queue_actor_spawn_in_zone(
                &mut pending,
                &mut spawner,
                &mut occupied_positions,
                &mut rng,
                &map_config,
                &map_geometry,
                &collision_world,
                server_gameplay_config.actors.settings.spawn_warning_secs,
                actor_physics,
                zone_idx,
                &zone.kind,
            );
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
    map_geometry: Res<MapGeometry>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    players: Query<&Position, With<PlayerMarker>>,
    actor_positions: Query<&Position, (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    // A zone gets one timer for all vacancies; later deaths do not restart it.
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
        occupied_positions.push(entry.pos);
    }
    let mut rng = rng();

    for zone_idx in due_zones {
        timers.0.remove(&zone_idx);
        let Some(zone) = map_config.actor_spawn_zones.get(zone_idx) else {
            continue;
        };
        let actor_config = gameplay_config.expect_actor(&zone.kind);
        let actor_physics = actor_config.physics();
        let live = live_by_zone[zone_idx];
        for _ in live..zone.count {
            queue_actor_spawn_in_zone(
                &mut pending,
                &mut spawner,
                &mut occupied_positions,
                &mut rng,
                &map_config,
                &map_geometry,
                &collision_world,
                server_gameplay_config.actors.settings.spawn_warning_secs,
                actor_physics,
                zone_idx,
                &zone.kind,
            );
        }
    }
}

pub(crate) fn expedite_actor_respawns(
    actors: &ActorMap,
    pending: &mut PendingActorSpawns,
    timers: &mut ActorRespawnTimers,
    map_config: &MapConfig,
    server_gameplay_config: &ServerGameplayConfig,
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
            spawn.remaining_secs = 0.0;
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
                timers.0.insert(zone_idx, 0.0);
                respawning += missing as usize;
            }
        }
    }

    respawning
}

// Ticks the beam-in warning windows and materializes due spawns at their
// reserved spot, unconditionally — a player squatting on it resolves via
// contact detonation on the next tick. Runs at the head of the network chain
// (see main.rs) so a pending entry's removal and its actor's appearance land
// in the same snapshot.
pub fn actors_pending_spawn_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    mut pending: ResMut<PendingActorSpawns>,
    time: Res<Time>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    map_settings: Res<MapSettings>,
) {
    let due = take_due_spawns(&mut pending.0, time.delta_secs());
    if due.is_empty() {
        return;
    }
    for spawn in due {
        let max_health = server_gameplay_config.combat.health.expect_actor(&spawn.kind).max;
        let movement = *map_settings.movement.expect_actor(&spawn.kind);
        materialize_actor(&mut commands, &mut actors, max_health, movement, spawn);
    }
}

fn take_due_spawns(pending: &mut Vec<PendingActorSpawn>, dt: f32) -> Vec<PendingActorSpawn> {
    for entry in pending.iter_mut() {
        entry.remaining_secs -= dt;
    }
    let (due, rest): (Vec<_>, Vec<_>) = pending.drain(..).partition(|entry| entry.remaining_secs <= 0.0);
    *pending = rest;
    due
}

// Reserve an id, spot, and heading for one actor and queue it for beam-in.
// The heading is rolled now so the client ghost and the materialized actor
// face the same way.
#[allow(clippy::too_many_arguments)]
fn queue_actor_spawn_in_zone(
    pending: &mut PendingActorSpawns,
    spawner: &mut ActorSpawner,
    occupied_positions: &mut Vec<Position>,
    rng: &mut ThreadRng,
    map_config: &MapConfig,
    map_geometry: &MapGeometry,
    collision_world: &CollisionWorld,
    warning_secs: f32,
    actor_physics: common::config::CharacterPhysicsConfig,
    zone_idx: usize,
    spawn_kind: &str,
) {
    let pos = generate_actor_spawn_position_in_zone(
        map_config,
        map_geometry,
        zone_idx,
        collision_world,
        occupied_positions,
        actor_physics,
    );
    occupied_positions.push(pos);

    pending.0.push(PendingActorSpawn {
        actor_id: spawner.allocate(),
        zone_idx,
        kind: spawn_kind.to_string(),
        pos,
        face_yaw: rng.random_range(0.0..TAU),
        remaining_secs: warning_secs,
    });
}

fn materialize_actor(
    commands: &mut Commands,
    actors: &mut ActorMap,
    max_health: f32,
    movement: ActorMovementConfig,
    spawn: PendingActorSpawn,
) {
    let move_intent = ActorMoveIntent::Idle;
    let entity = commands
        .spawn((
            ActorMarker,
            spawn.actor_id,
            movement,
            spawn.pos,
            move_intent,
            FaceYaw(spawn.face_yaw),
            CharacterVerticalVelocity::default(),
            Health(max_health),
        ))
        .id();

    actors.insert(spawn.actor_id, ActorInfo::new(entity, spawn.zone_idx, spawn.kind));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{ActorSpawnZone, CellGrid, EdgeGrid, LevelGrid};
    use common::protocol::ActorId;

    #[test]
    fn respawn_timer_starts_at_the_configured_delay() {
        let mut timers = ActorRespawnTimers::default();
        arm_actor_respawn(&mut timers, 3, 2.0);

        assert!(tick_actor_respawns(&mut timers, 1.0).is_empty());
        assert_eq!(tick_actor_respawns(&mut timers, 1.0), vec![3]);
    }

    #[test]
    fn another_vacancy_does_not_restart_an_active_zone_timer() {
        let mut timers = ActorRespawnTimers::default();
        arm_actor_respawn(&mut timers, 3, 2.0);
        assert!(tick_actor_respawns(&mut timers, 1.0).is_empty());

        arm_actor_respawn(&mut timers, 3, 2.0);

        assert_eq!(timers.0[&3], 1.0);
        assert_eq!(tick_actor_respawns(&mut timers, 1.0), vec![3]);
    }

    fn pending_spawn(id: u32, remaining_secs: f32) -> PendingActorSpawn {
        PendingActorSpawn {
            actor_id: ActorId(id),
            zone_idx: 0,
            kind: "zapper".to_string(),
            pos: Position::default(),
            face_yaw: 0.0,
            remaining_secs,
        }
    }

    #[test]
    fn expiring_selected_cooldowns_advances_pending_and_missing_slots() {
        let map_config = MapConfig {
            levels: vec![LevelGrid {
                cells: CellGrid::new(1, 1),
                edges: EdgeGrid::new(1, 1),
                barrier_edges: EdgeGrid::new(1, 1),
            }],
            actor_spawn_zones: vec![
                ActorSpawnZone {
                    level: 0,
                    cols: [0, 1],
                    rows: [0, 1],
                    kind: "mine".to_owned(),
                    count: 2,
                },
                ActorSpawnZone {
                    level: 0,
                    cols: [0, 1],
                    rows: [0, 1],
                    kind: "zapper".to_owned(),
                    count: 1,
                },
            ],
            player_spawn_zones: Vec::new(),
            placed_items: Vec::new(),
            pressure_plates: Vec::new(),
        };
        let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let mut mine = pending_spawn(1, 2.0);
        mine.kind = "mine".to_owned();
        let mut zapper = pending_spawn(2, 2.0);
        zapper.zone_idx = 1;
        let mut pending = PendingActorSpawns(vec![mine, zapper]);
        let mut timers = ActorRespawnTimers::default();
        timers.0.insert(0, 60.0);
        timers.0.insert(1, 120.0);

        let count = expedite_actor_respawns(
            &ActorMap::default(),
            &mut pending,
            &mut timers,
            &map_config,
            &config,
            Some("mine"),
        );

        assert_eq!(count, 2);
        assert_eq!(pending.0[0].remaining_secs, 0.0);
        assert_eq!(pending.0[1].remaining_secs, 2.0);
        assert_eq!(timers.0[&0], 0.0);
        assert_eq!(timers.0[&1], 120.0);
    }

    #[test]
    fn not_due_spawns_tick_down_and_stay_queued() {
        let mut pending = vec![pending_spawn(1, 2.0), pending_spawn(2, 0.5)];

        let due = take_due_spawns(&mut pending, 0.25);

        assert!(due.is_empty());
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].remaining_secs, 1.75);
        assert_eq!(pending[1].remaining_secs, 0.25);
    }

    #[test]
    fn due_spawns_drain_in_queue_order() {
        let mut pending = vec![pending_spawn(1, 0.1), pending_spawn(2, 5.0), pending_spawn(3, 0.2)];

        let due = take_due_spawns(&mut pending, 0.3);

        assert_eq!(
            due.iter().map(|spawn| spawn.actor_id).collect::<Vec<_>>(),
            vec![ActorId(1), ActorId(3)]
        );
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].actor_id, ActorId(2));
    }

    #[test]
    fn window_reaching_exactly_zero_is_due() {
        let mut pending = vec![pending_spawn(1, 0.5)];

        let due = take_due_spawns(&mut pending, 0.5);

        assert_eq!(due.len(), 1);
        assert!(pending.is_empty());
    }
}
