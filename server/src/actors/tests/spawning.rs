use super::*;
use crate::{
    actors::test_kinds::{self, BEAM, CONTACT, IMMOVABLE},
    map::{CellGrid, EdgeGrid, LevelGrid},
};
use bevy::ecs::system::RunSystemOnce;
use common::protocol::{ActorId, Carrier, CarrierId, MapLayout, SwitchId};

fn spawn_app(cols: i32, counts: &[u32], respawn_secs: Option<f32>) -> App {
    spawn_app_for(IMMOVABLE, cols, counts, respawn_secs)
}

fn spawn_app_for(kind: &str, cols: i32, counts: &[u32], respawn_secs: Option<f32>) -> App {
    let mut config = test_kinds::server_config();
    config
        .actors
        .kinds
        .get_mut(kind)
        .expect("actor kind missing")
        .respawn_secs = respawn_secs;
    let settings = config.maps[&config.default_map].settings.clone();
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
            switch_inverted: false,

            carrier: CarrierId::WORLD,
            level: 0,
            cols: [0, cols],
            rows: [0, 1],
            kind: kind.into(),
            count,
            switch: None,
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
        .init_resource::<PlateState>()
        .add_systems(Startup, actors_initial_spawn_system)
        .add_systems(Update, (actors_pending_spawn_system, actors_respawn_system).chain());
    app
}

fn reset(app: &mut App, scope: ActorRespawnScope) {
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut actors: ResMut<ActorMap>,
                  mut pending: ResMut<PendingActorSpawns>,
                  mut timers: ResMut<ActorRespawnTimers>,
                  map_config: Res<MapConfig>| {
                reset_actors(
                    &mut commands,
                    &mut actors,
                    &mut pending,
                    &mut timers,
                    &map_config,
                    scope,
                );
            },
        )
        .expect("reset system failed");
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
        .expect("live actor missing");
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
fn blocked_movable_spawn_waits_for_space_when_respawns_are_disabled() {
    let mut app = spawn_app_for(CONTACT, 1, &[1], None);
    app.world_mut().resource_mut::<MapConfig>().grids[0].levels[0]
        .cells
        .rows[0][0]
        .has_ramp = true;
    app.update();
    assert!(app.world().resource::<PendingActorSpawns>().0.is_empty());
    assert_eq!(
        app.world().resource::<ActorRespawnTimers>().0[&0],
        ActorRespawnState::WaitingForSpace
    );
    app.world_mut().resource_mut::<MapConfig>().grids[0].levels[0]
        .cells
        .rows[0][0]
        .has_ramp = false;
    app.update();
    assert_eq!(app.world().resource::<PendingActorSpawns>().0.len(), 1);
    assert!(app.world().resource::<ActorRespawnTimers>().0.is_empty());
}

#[test]
fn resetting_every_actor_keeps_peace() {
    let mut app = spawn_app(1, &[1], Some(1.0));
    app.update();
    app.world_mut().resource_mut::<ActorMap>().set_peaceful(true);
    reset(&mut app, ActorRespawnScope::All);
    let actors = app.world().resource::<ActorMap>();
    assert!(actors.peaceful);
    assert_eq!(actors.values().count(), 0);
    assert!(app.world().resource::<PendingActorSpawns>().0.is_empty());
}

#[test]
fn blocked_reset_refills_retry_as_soon_as_space_clears() {
    for scope in [ActorRespawnScope::Dead, ActorRespawnScope::All] {
        let mut app = spawn_app_for(CONTACT, 1, &[1], Some(90.0));
        app.update();
        let spawn = &app.world().resource::<PendingActorSpawns>().0[0];
        let id = spawn.actor_id;
        let due_tick = spawn.due_tick;
        app.world_mut().resource_mut::<ServerTick>().0 = due_tick;
        app.update();
        if scope == ActorRespawnScope::Dead {
            let removed = app
                .world_mut()
                .resource_mut::<ActorMap>()
                .remove(&id)
                .expect("live actor missing");
            app.world_mut().despawn(removed.entity);
        }
        app.world_mut().resource_mut::<MapConfig>().grids[0].levels[0]
            .cells
            .rows[0][0]
            .has_ramp = true;
        reset(&mut app, scope);
        for _ in 0..3 {
            app.update();
            assert!(app.world().resource::<PendingActorSpawns>().0.is_empty());
        }

        app.world_mut().resource_mut::<MapConfig>().grids[0].levels[0]
            .cells
            .rows[0][0]
            .has_ramp = false;
        app.update();

        assert_eq!(
            app.world().resource::<PendingActorSpawns>().0.len(),
            1,
            "scope {scope:?}"
        );
        assert!(app.world().resource::<ActorRespawnTimers>().0.is_empty());
    }
}

#[test]
fn blocked_automatic_movable_spawn_keeps_its_retry_delay() {
    let mut app = spawn_app_for(CONTACT, 1, &[1], Some(90.0));
    app.world_mut().resource_mut::<MapConfig>().grids[0].levels[0]
        .cells
        .rows[0][0]
        .has_ramp = true;
    app.update();
    assert!(app.world().resource::<PendingActorSpawns>().0.is_empty());
    app.world_mut().resource_mut::<MapConfig>().grids[0].levels[0]
        .cells
        .rows[0][0]
        .has_ramp = false;
    app.update();
    assert!(app.world().resource::<PendingActorSpawns>().0.is_empty());
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
        kind: BEAM.to_string(),
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
        switch_inverted: false,

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
        switch: None,
    };
    let mut carriers = Carriers::from_layout(&MapLayout {
        carriers: vec![carrier],
        ..MapLayout::default()
    });
    let mut spawn = pending_spawn(1, 90);
    spawn.carrier = CarrierId(1);
    spawn.pos = Position { x: 1.0, y: 0.0, z: 2.0 };
    assert_eq!(spawn.world_position(&carriers), Position { x: 1.0, y: 0.0, z: 2.0 });

    carriers.advance(6, &PlateState::default());

    assert_eq!(spawn.world_position(&carriers), Position { x: 7.0, y: 0.0, z: 2.0 });
}

#[test]
fn expiring_selected_cooldowns_advances_pending_and_missing_slots() {
    let map_config = MapConfig {
        actor_spawn_zones: vec![
            ActorSpawnZone {
                switch_inverted: false,

                carrier: CarrierId::WORLD,
                level: 0,
                cols: [0, 1],
                rows: [0, 1],
                kind: CONTACT.to_owned(),
                count: 2,
                switch: None,
            },
            ActorSpawnZone {
                switch_inverted: false,

                carrier: CarrierId::WORLD,
                level: 0,
                cols: [0, 1],
                rows: [0, 1],
                kind: BEAM.to_owned(),
                count: 1,
                switch: None,
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
    let config = test_kinds::server_config();
    let mut contact = pending_spawn(1, 60);
    contact.kind = CONTACT.to_owned();
    let mut beam = pending_spawn(2, 60);
    beam.zone_idx = 1;
    let mut pending = PendingActorSpawns(vec![contact, beam]);
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
        Some(CONTACT),
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

const GUARDS: SwitchId = SwitchId(0);

// A one-zone app whose zone is operated by `GUARDS`, switched off.
fn switched_app(respawn_secs: Option<f32>, count: u32) -> App {
    let mut app = spawn_app_for(CONTACT, 3, &[count], respawn_secs);
    app.world_mut().resource_mut::<MapConfig>().actor_spawn_zones[0].switch = Some(GUARDS);
    app
}

fn set_switch(app: &mut App, active: bool) {
    let mut plates = app.world_mut().resource_mut::<PlateState>();
    plates.active_switches = if active { vec![GUARDS] } else { Vec::new() };
}

fn zone_state(app: &App) -> Option<ActorRespawnState> {
    app.world().resource::<ActorRespawnTimers>().0.get(&0).copied()
}

fn pending_count(app: &App) -> usize {
    app.world().resource::<PendingActorSpawns>().0.len()
}

fn materialize_pending(app: &mut App) {
    let due = app
        .world()
        .resource::<PendingActorSpawns>()
        .0
        .iter()
        .map(|spawn| spawn.due_tick)
        .max()
        .expect("nothing pending to materialize");
    app.world_mut().resource_mut::<ServerTick>().0 = due;
    app.update();
    assert_eq!(pending_count(app), 0);
}

fn destroy_one(app: &mut App) {
    let id = *app
        .world()
        .resource::<ActorMap>()
        .iter()
        .next()
        .expect("no live actor to destroy")
        .0;
    let removed = app
        .world_mut()
        .resource_mut::<ActorMap>()
        .remove(&id)
        .expect("live actor missing");
    app.world_mut().despawn(removed.entity);
}

#[test]
fn a_switched_zone_spawns_nothing_until_its_switch_turns_on() {
    let mut app = switched_app(Some(0.0), 2);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(pending_count(&app), 0, "no initial fill");
    assert_eq!(zone_state(&app), Some(ActorRespawnState::Inactive));

    set_switch(&mut app, true);
    app.update();
    assert_eq!(pending_count(&app), 2, "a zero respawn time fills on the next pass");
    assert_eq!(zone_state(&app), None);
    app.update();
    assert_eq!(pending_count(&app), 2, "an active full zone queues nothing more");
}

#[test]
fn an_activation_starts_the_kinds_countdown() {
    let mut app = switched_app(Some(1000.0), 1);
    app.update();
    set_switch(&mut app, true);
    app.update();
    assert_eq!(pending_count(&app), 0);
    assert!(
        matches!(zone_state(&app), Some(ActorRespawnState::Cooldown(secs)) if secs > 999.0),
        "{:?}",
        zone_state(&app)
    );

    app.world_mut()
        .resource_mut::<ActorRespawnTimers>()
        .0
        .insert(0, ActorRespawnState::Cooldown(0.0));
    app.update();
    assert_eq!(pending_count(&app), 1, "the countdown's end fills the zone");
}

#[test]
fn a_switched_off_zone_drops_its_countdown_and_ignores_kills() {
    let mut app = switched_app(Some(1000.0), 1);
    app.update();
    set_switch(&mut app, true);
    app.update();
    assert!(matches!(zone_state(&app), Some(ActorRespawnState::Cooldown(_))));
    set_switch(&mut app, false);
    app.update();
    assert_eq!(
        zone_state(&app),
        Some(ActorRespawnState::Inactive),
        "the countdown is dropped"
    );
    set_switch(&mut app, true);
    app.update();
    assert!(
        matches!(zone_state(&app), Some(ActorRespawnState::Cooldown(secs)) if secs > 999.0),
        "a fresh countdown starts on reactivation: {:?}",
        zone_state(&app)
    );

    let mut app = switched_app(Some(0.0), 1);
    app.update();
    set_switch(&mut app, true);
    app.update();
    materialize_pending(&mut app);
    assert_eq!(app.world().resource::<ActorMap>().values().count(), 1);
    set_switch(&mut app, false);
    app.update();
    destroy_one(&mut app);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(pending_count(&app), 0, "a kill while off arms nothing");
    assert_eq!(zone_state(&app), Some(ActorRespawnState::Inactive));
    set_switch(&mut app, true);
    app.update();
    assert_eq!(pending_count(&app), 1, "reactivation refills the vacancy");
}

#[test]
fn an_active_switched_zone_refills_kills_on_its_kinds_timer() {
    let mut app = switched_app(Some(0.0), 1);
    app.update();
    set_switch(&mut app, true);
    app.update();
    materialize_pending(&mut app);
    destroy_one(&mut app);
    app.update();
    assert_eq!(
        pending_count(&app),
        1,
        "a zero respawn time refills at once while active"
    );
}

#[test]
fn a_reset_leaves_an_inactive_switched_zone_empty_and_refills_an_active_one() {
    let mut app = switched_app(Some(0.0), 1);
    app.update();
    reset(&mut app, ActorRespawnScope::All);
    app.update();
    assert_eq!(pending_count(&app), 0);
    assert_eq!(zone_state(&app), Some(ActorRespawnState::Inactive));

    set_switch(&mut app, true);
    app.update();
    materialize_pending(&mut app);
    reset(&mut app, ActorRespawnScope::All);
    assert_eq!(app.world().resource::<ActorMap>().values().count(), 0);
    app.update();
    assert_eq!(
        pending_count(&app),
        1,
        "an active zone refills after a reset like any other"
    );
}

#[test]
fn expediting_respawns_skips_a_switched_off_zone() {
    let mut app = switched_app(Some(0.0), 1);
    app.update();
    let expedited = app
        .world_mut()
        .run_system_once(
            |actors: Res<ActorMap>,
             mut pending: ResMut<PendingActorSpawns>,
             mut timers: ResMut<ActorRespawnTimers>,
             map_config: Res<MapConfig>,
             config: Res<ServerGameplayConfig>,
             tick: Res<ServerTick>| {
                expedite_actor_respawns(&actors, &mut pending, &mut timers, &map_config, &config, tick.0, None)
            },
        )
        .expect("expedite system failed");
    assert_eq!(expedited, 0);
    app.update();
    assert_eq!(pending_count(&app), 0);
    assert_eq!(zone_state(&app), Some(ActorRespawnState::Inactive));
}

#[test]
fn an_inverted_zone_spawns_while_its_pressure_plate_kind_is_off() {
    let mut app = switched_app(Some(0.0), 2);
    app.world_mut().resource_mut::<MapConfig>().actor_spawn_zones[0].switch_inverted = true;
    set_switch(&mut app, true);
    app.update();
    assert_eq!(pending_count(&app), 0);
    assert_eq!(zone_state(&app), Some(ActorRespawnState::Inactive));
    set_switch(&mut app, false);
    app.update();
    assert_eq!(pending_count(&app), 2);
}
