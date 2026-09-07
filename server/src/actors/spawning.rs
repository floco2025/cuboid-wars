use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};
use std::f32::consts::TAU;

use crate::{
    actors::{
        ActorCrushed, ActorInfo, ActorMap, ActorRespawnState, ActorRespawnTimers, ActorSpawner, PendingActorSpawn,
        PendingActorSpawns,
    },
    characters::generate_actor_spawn_position_in_zone,
    config::{ActorRespawnScope, ServerGameplayConfig},
    map::{ActorSpawnZone, MapConfig},
};
use common::{
    config::{ActorGameplayConfig, ActorMovementConfig},
    map::Carriers,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{
        ActorAnchor, ActorMarker, ActorMoveIntent, FaceYaw, Health, MapSettings, PlayerMarker, Position, ServerTick,
        sequence_is_newer,
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
        *actors = ActorMap::default();
        pending.0.clear();
    }
    timers.0.clear();
    timers
        .0
        .extend((0..map_config.actor_spawn_zones.len()).map(|zone_idx| (zone_idx, ActorRespawnState::WaitingForSpace)));
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
            ActorRespawnState::WaitingForSpace => Some(*zone_idx),
        })
        .collect()
}

// Startup-only: fill every spawn zone to its `count`. Runs once when the
// world boots, irrespective of `respawn_secs` — initial fill is universal.
// Spawns are queued, not spawned: each waits out its beam-in warning window
// in `PendingActorSpawns` before `actors_pending_spawn_system` materializes it.
pub fn actors_initial_spawn_system(
    mut pending: ResMut<PendingActorSpawns>,
    mut spawner: ResMut<ActorSpawner>,
    mut timers: ResMut<ActorRespawnTimers>,
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    tick: Res<ServerTick>,
    players: Query<&Position, With<PlayerMarker>>,
) {
    // Avoid spawning on top of any players that may already exist (none, in
    // practice, at Startup — but cheap and consistent with the respawn path).
    let mut occupied_positions: Vec<Position> = players.iter().copied().collect();
    let mut rng = rng();

    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        queue_zone_slots(
            &mut pending,
            &mut spawner,
            &mut timers,
            &mut occupied_positions,
            &mut rng,
            &map_config,
            &carriers,
            &collision_world,
            &server_gameplay_config,
            tick.0,
            zone_idx,
            zone,
            zone.count,
        );
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
    tick: Res<ServerTick>,
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
        occupied_positions.push(entry.world_position(&carriers));
    }
    let mut rng = rng();

    for zone_idx in due_zones {
        let Some(zone) = map_config.actor_spawn_zones.get(zone_idx) else {
            timers.0.remove(&zone_idx);
            continue;
        };
        let missing = zone.count.saturating_sub(live_by_zone[zone_idx]);
        queue_zone_slots(
            &mut pending,
            &mut spawner,
            &mut timers,
            &mut occupied_positions,
            &mut rng,
            &map_config,
            &carriers,
            &collision_world,
            &server_gameplay_config,
            tick.0,
            zone_idx,
            zone,
            missing,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn queue_zone_slots(
    pending: &mut PendingActorSpawns,
    spawner: &mut ActorSpawner,
    timers: &mut ActorRespawnTimers,
    occupied_positions: &mut Vec<Position>,
    rng: &mut ThreadRng,
    map_config: &MapConfig,
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    server_gameplay_config: &ServerGameplayConfig,
    tick: u32,
    zone_idx: usize,
    zone: &ActorSpawnZone,
    missing: u32,
) {
    let kind_config = server_gameplay_config.expect_actor(&zone.kind);
    for _ in 0..missing {
        let queued = queue_actor_spawn_in_zone(
            pending,
            spawner,
            occupied_positions,
            rng,
            map_config,
            carriers,
            collision_world,
            tick,
            server_gameplay_config.actors.settings.spawn_warning_ticks(),
            &kind_config.character,
            zone_idx,
            zone,
        );
        if !queued {
            if kind_config.character.immovable || timers.0.get(&zone_idx) == Some(&ActorRespawnState::WaitingForSpace) {
                let previous = timers.0.insert(zone_idx, ActorRespawnState::WaitingForSpace);
                if previous != Some(ActorRespawnState::WaitingForSpace) {
                    warn!(
                        "actor spawn zone {zone_idx} on carrier {} has no clear spot for {:?}; waiting for space",
                        zone.carrier.0, zone.kind
                    );
                }
                return;
            }
            warn!(
                "actor spawn zone {zone_idx} on carrier {} has no clear spot for a {:?}; retrying after its respawn time",
                zone.carrier.0, zone.kind
            );
            timers.0.remove(&zone_idx);
            if let Some(respawn_secs) = kind_config.respawn_secs {
                arm_actor_respawn(timers, zone_idx, respawn_secs);
            }
            return;
        }
    }
    timers.0.remove(&zone_idx);
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
                if let ActorRespawnState::Cooldown(remaining_secs) = state {
                    *remaining_secs = 0.0;
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
        let kind = server_gameplay_config.expect_actor(&spawn.kind);
        let movement = (!kind.character.immovable).then(|| *map_settings.movement.expect_actor(&spawn.kind));
        materialize_actor(&mut commands, &mut actors, &carriers, max_health, movement, spawn);
    }
}

fn take_due_spawns(pending: &mut Vec<PendingActorSpawn>, tick: u32) -> Vec<PendingActorSpawn> {
    let (due, rest): (Vec<_>, Vec<_>) = pending
        .drain(..)
        .partition(|entry| !sequence_is_newer(entry.due_tick, tick));
    *pending = rest;
    due
}

// Reserve an id, spot, and heading for one actor and queue it for beam-in.
// The heading is rolled now so the client ghost and the materialized actor
// face the same way. The spot is kept in the zone's carrier frame, so it
// rides the carrier through the warning window, which runs `warning_ticks`
// from `tick`. False when the zone has no clear spot.
#[allow(clippy::too_many_arguments)]
fn queue_actor_spawn_in_zone(
    pending: &mut PendingActorSpawns,
    spawner: &mut ActorSpawner,
    occupied_positions: &mut Vec<Position>,
    rng: &mut ThreadRng,
    map_config: &MapConfig,
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    tick: u32,
    warning_ticks: u32,
    actor_config: &ActorGameplayConfig,
    zone_idx: usize,
    zone: &ActorSpawnZone,
) -> bool {
    let Some(pos) = generate_actor_spawn_position_in_zone(
        map_config,
        carriers,
        zone,
        collision_world,
        occupied_positions,
        actor_config,
    ) else {
        return false;
    };
    occupied_positions.push(pos);

    pending.0.push(PendingActorSpawn {
        actor_id: spawner.allocate(),
        zone_idx,
        kind: zone.kind.clone(),
        carrier: zone.carrier,
        pos: carriers.pose(zone.carrier).inverse_transform_position(&pos),
        face_yaw: rng.random_range(0.0..TAU),
        reserved_tick: tick,
        due_tick: tick.wrapping_add(warning_ticks),
    });
    true
}

fn materialize_actor(
    commands: &mut Commands,
    actors: &mut ActorMap,
    carriers: &Carriers,
    max_health: f32,
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
            Health(max_health),
            ActorCrushed::default(),
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
mod tests {
    use super::*;
    use crate::map::{CellGrid, EdgeGrid, LevelGrid};
    use common::protocol::{ActorId, Carrier, CarrierId, MapLayout};

    fn spawn_app(cols: i32, counts: &[u32], respawn_secs: Option<f32>) -> App {
        let mut config = ServerGameplayConfig::load_default().expect("gameplay config rejected");
        config
            .actors
            .kinds
            .get_mut("turret")
            .expect("turret kind missing")
            .respawn_secs = respawn_secs;
        let settings = config.maps["hotel"].settings.clone();
        let mut cells = CellGrid::new(cols, 1);
        for cell in &mut cells.rows[0] {
            cell.has_floor = true;
        }
        let mut map = MapConfig::for_grid(
            vec![LevelGrid {
                cells,
                edges: EdgeGrid::new(cols, 1),
                barrier_edges: EdgeGrid::new(cols, 1),
            }],
            crate::test_geometry::geometry(cols, 1),
        );
        map.actor_spawn_zones = counts
            .iter()
            .map(|&count| ActorSpawnZone {
                carrier: CarrierId::WORLD,
                level: 0,
                cols: [0, cols],
                rows: [0, 1],
                kind: "turret".into(),
                count,
            })
            .collect();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(config)
            .insert_resource(settings)
            .insert_resource(map)
            .insert_resource(CollisionWorld::from_map_layout(
                &MapLayout::default(),
                &Default::default(),
            ))
            .init_resource::<Carriers>()
            .init_resource::<ActorMap>()
            .init_resource::<ActorRespawnTimers>()
            .init_resource::<ActorSpawner>()
            .init_resource::<PendingActorSpawns>()
            .init_resource::<ServerTick>()
            .add_systems(Startup, actors_initial_spawn_system)
            .add_systems(Update, (actors_pending_spawn_system, actors_respawn_system).chain());
        app
    }

    #[test]
    fn overlapping_zones_reserve_pending_and_live_centers_then_fill_a_vacancy() {
        let mut app = spawn_app(2, &[2, 1], Some(180.0));
        app.update();
        let pending = &app.world().resource::<PendingActorSpawns>().0;
        assert_eq!(pending.len(), 2);
        assert_ne!(pending[0].pos, pending[1].pos);
        let first_id = pending[0].actor_id;
        let freed_pos = pending[0].pos;
        let due_tick = pending[0].due_tick;
        assert_eq!(
            app.world().resource::<ActorRespawnTimers>().0[&1],
            ActorRespawnState::WaitingForSpace
        );
        for _ in 0..10 {
            app.update();
        }
        assert_eq!(app.world().resource::<PendingActorSpawns>().0.len(), 2);
        assert_eq!(app.world().resource::<ActorSpawner>().next_id, 2);
        app.world_mut().resource_mut::<ServerTick>().0 = due_tick;
        app.update();
        assert_eq!(app.world().resource::<ActorMap>().values().count(), 2);
        assert!(app.world().resource::<PendingActorSpawns>().0.is_empty());
        assert_eq!(
            app.world().resource::<ActorRespawnTimers>().0[&1],
            ActorRespawnState::WaitingForSpace
        );
        let removed = app
            .world_mut()
            .resource_mut::<ActorMap>()
            .remove(&first_id)
            .expect("live turret missing");
        app.world_mut().despawn(removed.entity);
        app.update();
        let pending = &app.world().resource::<PendingActorSpawns>().0;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].zone_idx, 1);
        assert_eq!(pending[0].pos, freed_pos);
        assert!(!app.world().resource::<ActorRespawnTimers>().0.contains_key(&1));
    }

    #[test]
    fn blocked_initial_immovable_spawn_retries_even_when_respawns_are_disabled() {
        let mut app = spawn_app(1, &[1], None);
        let player = app.world_mut().spawn((PlayerMarker, Position::default())).id();
        app.update();
        assert!(app.world().resource::<PendingActorSpawns>().0.is_empty());
        assert_eq!(
            app.world().resource::<ActorRespawnTimers>().0[&0],
            ActorRespawnState::WaitingForSpace
        );
        app.world_mut().despawn(player);
        app.update();
        let pending = &app.world().resource::<PendingActorSpawns>().0;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].pos, Position::default());
        assert!(app.world().resource::<ActorRespawnTimers>().0.is_empty());
    }

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

        assert_eq!(timers.0[&3], ActorRespawnState::Cooldown(1.0));
        assert_eq!(tick_actor_respawns(&mut timers, 1.0), vec![3]);
    }

    fn pending_spawn(id: u32, due_tick: u32) -> PendingActorSpawn {
        PendingActorSpawn {
            actor_id: ActorId(id),
            zone_idx: 0,
            kind: "zapper".to_string(),
            carrier: CarrierId::WORLD,
            pos: Position::default(),
            face_yaw: 0.0,
            reserved_tick: 0,
            due_tick,
        }
    }

    #[test]
    fn a_pending_spawn_on_a_carrier_materializes_where_the_carrier_is_now() {
        let carrier = Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: Position { x: 0.0, y: 0.0, z: 0.0 },
            to: Position {
                x: 12.0,
                y: 0.0,
                z: 0.0,
            },
            travel_ticks: 12,
            pause_ticks: 0,
            phase_ticks: 0,
        };
        let mut carriers = Carriers::from_layout(&MapLayout {
            carriers: vec![carrier],
            ..MapLayout::default()
        });
        let mut spawn = pending_spawn(1, 90);
        spawn.carrier = CarrierId(1);
        spawn.pos = Position { x: 1.0, y: 0.0, z: 2.0 };
        assert_eq!(spawn.world_position(&carriers), Position { x: 1.0, y: 0.0, z: 2.0 });

        carriers.advance(6);

        assert_eq!(spawn.world_position(&carriers), Position { x: 7.0, y: 0.0, z: 2.0 });
    }

    #[test]
    fn expiring_selected_cooldowns_advances_pending_and_missing_slots() {
        let map_config = MapConfig {
            actor_spawn_zones: vec![
                ActorSpawnZone {
                    carrier: CarrierId::WORLD,
                    level: 0,
                    cols: [0, 1],
                    rows: [0, 1],
                    kind: "mine".to_owned(),
                    count: 2,
                },
                ActorSpawnZone {
                    carrier: CarrierId::WORLD,
                    level: 0,
                    cols: [0, 1],
                    rows: [0, 1],
                    kind: "zapper".to_owned(),
                    count: 1,
                },
            ],
            ..MapConfig::for_grid(
                vec![LevelGrid {
                    cells: CellGrid::new(1, 1),
                    edges: EdgeGrid::new(1, 1),
                    barrier_edges: EdgeGrid::new(1, 1),
                }],
                crate::test_geometry::geometry(1, 1),
            )
        };
        let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let mut mine = pending_spawn(1, 60);
        mine.kind = "mine".to_owned();
        let mut zapper = pending_spawn(2, 60);
        zapper.zone_idx = 1;
        let mut pending = PendingActorSpawns(vec![mine, zapper]);
        let mut timers = ActorRespawnTimers::default();
        timers.0.insert(0, ActorRespawnState::Cooldown(60.0));
        timers.0.insert(1, ActorRespawnState::Cooldown(120.0));

        let count = expedite_actor_respawns(
            &ActorMap::default(),
            &mut pending,
            &mut timers,
            &map_config,
            &config,
            100,
            Some("mine"),
        );

        assert_eq!(count, 2);
        assert_eq!(pending.0[0].due_tick, 100);
        assert_eq!(pending.0[1].due_tick, 60);
        assert_eq!(timers.0[&0], ActorRespawnState::Cooldown(0.0));
        assert_eq!(timers.0[&1], ActorRespawnState::Cooldown(120.0));
    }

    #[test]
    fn spawns_before_their_due_tick_stay_queued() {
        let mut pending = vec![pending_spawn(1, 60), pending_spawn(2, 15)];

        let due = take_due_spawns(&mut pending, 7);

        assert!(due.is_empty());
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn due_spawns_drain_in_queue_order() {
        let mut pending = vec![pending_spawn(1, 3), pending_spawn(2, 150), pending_spawn(3, 6)];

        let due = take_due_spawns(&mut pending, 9);

        assert_eq!(
            due.iter().map(|spawn| spawn.actor_id).collect::<Vec<_>>(),
            vec![ActorId(1), ActorId(3)]
        );
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].actor_id, ActorId(2));
    }

    #[test]
    fn a_spawn_is_due_on_its_due_tick() {
        let mut pending = vec![pending_spawn(1, 15)];

        assert!(take_due_spawns(&mut pending, 14).is_empty());
        let due = take_due_spawns(&mut pending, 15);

        assert_eq!(due.len(), 1);
        assert!(pending.is_empty());
    }
}
