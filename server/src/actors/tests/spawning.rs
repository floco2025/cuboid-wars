use super::*;
use crate::{
    actors::test_kinds::{self, BEAM, CONTACT, IMMOVABLE},
    map::{CellGrid, CheckpointResponse, EdgeGrid, LevelGrid},
    players::{PlayerCheckpoint, PlayerInfo},
    test_geometry::{LEVEL_HEIGHT, geometry},
};
use bevy::{ecs::system::RunSystemOnce, time::TimeUpdateStrategy};
use common::{
    config::ActorLocomotion,
    protocol::{ActorId, CarrierId, Checkpoint, CheckpointKind, MapLayout, PlayerId, SwitchId, Wall},
};
use crossbeam_channel::unbounded;
use std::time::Duration;

const GUARDS: SwitchId = SwitchId(0);

fn spawn_app(cols: i32, counts: &[u32], respawn_secs: Option<f32>) -> App {
    spawn_app_for(IMMOVABLE, cols, counts, respawn_secs)
}

fn spawn_app_for(kind: &str, cols: i32, counts: &[u32], respawn_secs: Option<f32>) -> App {
    let config = test_kinds::server_config();
    let settings = config.settings.clone();
    let mut map = MapConfig::for_grid(vec![floored_row(cols)], geometry(cols, 1));
    map.actor_spawn_zones = counts
        .iter()
        .map(|&count| ActorSpawnZone {
            count: vec![count],
            respawn_secs,
            beam_in_secs: 3.0,
            ..test_kinds::spawn_zone(kind, [0, cols], [0, 1])
        })
        .collect();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(config)
        .insert_resource(settings)
        .insert_resource(map)
        .insert_resource(CollisionWorld::from_map_layout(&MapLayout::default()))
        .init_resource::<Carriers>()
        .init_resource::<ActorMap>()
        .init_resource::<PlayerMap>()
        .init_resource::<ActorSpawner>()
        .init_resource::<PendingActorSpawns>()
        .init_resource::<ServerTick>()
        .init_resource::<SwitchState>()
        .init_resource::<MapLayout>()
        .add_systems(Update, (actors_pending_spawn_system, actors_respawn_system).chain());
    app
}

fn floored_row(cols: i32) -> LevelGrid {
    let mut cells = CellGrid::new(cols, 1);
    for cell in &mut cells.rows[0] {
        cell.has_floor = true;
    }
    LevelGrid {
        cells,
        edges: EdgeGrid::new(cols, 1),
    }
}

// A one-zone app whose zone is operated by `GUARDS`, switched off.
fn switched_app(respawn_secs: Option<f32>, count: u32) -> App {
    let mut app = spawn_app_for(CONTACT, 3, &[count], respawn_secs);
    let zone = &mut app.world_mut().resource_mut::<MapConfig>().actor_spawn_zones[0];
    zone.switch = Some(GUARDS);
    zone.initially_on = false;
    app
}

fn scaled_app(cols: i32, counts: &[u32], respawn_secs: Option<f32>) -> App {
    let mut app = spawn_app(cols, &[counts[0]], respawn_secs);
    app.world_mut().resource_mut::<MapConfig>().actor_spawn_zones[0].count = counts.to_vec();
    add_player(&mut app, 1, true);
    app
}

// A one-zone app on a three-checkpoint course, its zone ending at checkpoint `until`.
fn course_app(until: u32, on_checkpoint: CheckpointResponse, respawn_secs: Option<f32>) -> App {
    let mut app = spawn_app_for(CONTACT, 3, &[1], respawn_secs);
    {
        let mut map = app.world_mut().resource_mut::<MapConfig>();
        map.actor_spawn_zones[0].until_checkpoint = Some(until);
        map.actor_spawn_zones[0].on_checkpoint = on_checkpoint;
    }
    app.world_mut().resource_mut::<MapLayout>().checkpoints = (1..=3)
        .map(|number| Checkpoint {
            kind: CheckpointKind::Individual,
            number,
            carrier: CarrierId::WORLD,
            level: 0,
            cols: [0, 1],
            rows: [0, 1],
            min_x: 0.0,
            max_x: 1.0,
            min_z: 0.0,
            max_z: 1.0,
            y: 0.0,
        })
        .collect();
    app
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

fn blocked(app: &App, zone_idx: usize) -> bool {
    app.world().resource::<ActorSpawner>().blocked.contains(&zone_idx)
}

// The remaining delays of zone 0's vacated slots.
fn refills(app: &App) -> Vec<Option<f32>> {
    app.world()
        .resource::<ActorSpawner>()
        .refills
        .get(&0)
        .cloned()
        .unwrap_or_default()
}

fn pending_count(app: &App) -> usize {
    app.world().resource::<PendingActorSpawns>().0.len()
}

fn live_count(app: &App) -> usize {
    app.world().resource::<ActorMap>().values().count()
}

fn reset(app: &mut App, scope: ActorRespawnScope) {
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut actors: ResMut<ActorMap>,
                  mut pending: ResMut<PendingActorSpawns>,
                  mut spawner: ResMut<ActorSpawner>| {
                reset_actors(&mut commands, &mut actors, &mut pending, &mut spawner, scope);
            },
        )
        .expect("reset system failed");
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

fn destroy(app: &mut App, id: ActorId) {
    let removed = app
        .world_mut()
        .resource_mut::<ActorMap>()
        .remove(&id)
        .expect("live actor missing");
    app.world_mut().despawn(removed.entity);
}

fn destroy_one(app: &mut App) {
    let id = *app
        .world()
        .resource::<ActorMap>()
        .iter()
        .next()
        .expect("no live actor to destroy")
        .0;
    destroy(app, id);
}

fn expire_countdown(app: &mut App) {
    for secs in app
        .world_mut()
        .resource_mut::<ActorSpawner>()
        .refills
        .entry(0)
        .or_default()
        .iter_mut()
        .flatten()
    {
        *secs = 0.0;
    }
}

fn set_ramps(app: &mut App, has_ramp: bool) {
    for cell in &mut app.world_mut().resource_mut::<MapConfig>().grids[0].levels[0]
        .cells
        .rows[0]
    {
        cell.has_ramp = has_ramp;
    }
}

fn set_switch(app: &mut App, active: bool) {
    let mut switch_state = app.world_mut().resource_mut::<SwitchState>();
    switch_state.active_switches = if active { vec![GUARDS] } else { Vec::new() };
}

fn make_beam_kind_fly(app: &mut App) {
    app.world_mut()
        .resource_mut::<ServerGameplayConfig>()
        .actors
        .get_mut(BEAM)
        .expect("test beam kind missing")
        .character
        .locomotion = ActorLocomotion::Flying;
}

fn set_walls(app: &mut App, walls: Vec<Wall>) {
    app.world_mut()
        .insert_resource(CollisionWorld::from_map_layout(&MapLayout { walls, ..default() }));
}

fn add_player(app: &mut App, id: u32, logged_in: bool) {
    let entity = app.world_mut().spawn_empty().id();
    let (channel, _) = unbounded();
    let mut info = PlayerInfo::new(entity, channel);
    info.connection.logged_in = logged_in;
    app.world_mut().resource_mut::<PlayerMap>().insert(PlayerId(id), info);
}

fn leave_player(app: &mut App, id: u32) {
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&PlayerId(id))
        .expect("test player")
        .connection
        .logged_in = false;
}

// Logs `player` in if needed and saves checkpoint `number` for it.
fn reach(app: &mut App, player: PlayerId, number: u32) {
    if app.world().resource::<PlayerMap>().get(&player).is_none() {
        add_player(app, player.0, true);
    }
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&player)
        .expect("player missing")
        .session
        .checkpoint = PlayerCheckpoint::numbered(number);
}

#[test]
fn multilevel_zone_shares_its_count_and_fills_both_floors_when_needed() {
    let mut app = spawn_app(1, &[2], None);
    {
        let mut map = app.world_mut().resource_mut::<MapConfig>();
        let upper = map.grids[0].levels[0].clone();
        map.grids[0].levels.push(upper);
        map.actor_spawn_zones[0].levels = 2;
    }
    app.update();
    let pending = app.world().resource::<PendingActorSpawns>();
    assert_eq!(pending.0.len(), 2);
    let mut levels: Vec<_> = pending.0.iter().map(|spawn| spawn.pos.y).collect();
    levels.sort_by(f32::total_cmp);
    assert_eq!(levels, vec![0.0, LEVEL_HEIGHT]);
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
    assert!(blocked(&app, 1));
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(pending_count(&app), 2);
    assert_eq!(app.world().resource::<ActorSpawner>().next_id, 2);
    app.world_mut().resource_mut::<ServerTick>().0 = due_tick;
    app.update();
    assert_eq!(live_count(&app), 2);
    assert_eq!(pending_count(&app), 0);
    assert!(blocked(&app, 1));
    destroy(&mut app, first_id);
    app.update();
    let pending = &app.world().resource::<PendingActorSpawns>().0;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].zone_idx, 1);
    assert_eq!(pending[0].pos, freed_pos);
    assert!(!blocked(&app, 1));
}

#[test]
fn blocked_initial_immovable_spawn_retries_even_when_respawns_are_disabled() {
    let mut app = spawn_app(1, &[1], None);
    let player = app.world_mut().spawn((PlayerMarker, Position::default())).id();
    app.update();
    assert_eq!(pending_count(&app), 0);
    assert!(blocked(&app, 0));
    app.world_mut().despawn(player);
    app.update();
    let pending = &app.world().resource::<PendingActorSpawns>().0;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].pos, Position::default());
    assert!(!blocked(&app, 0));
}

#[test]
fn resetting_every_actor_keeps_peace() {
    let mut app = spawn_app(1, &[1], Some(1.0));
    app.update();
    app.world_mut().resource_mut::<ActorMap>().set_peaceful(true);
    reset(&mut app, ActorRespawnScope::All);
    assert!(app.world().resource::<ActorMap>().peaceful);
    assert_eq!(live_count(&app), 0);
    assert_eq!(pending_count(&app), 0);
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
            destroy(&mut app, id);
        }
        set_ramps(&mut app, true);
        reset(&mut app, scope);
        for _ in 0..3 {
            app.update();
            assert_eq!(pending_count(&app), 0);
        }

        set_ramps(&mut app, false);
        app.update();

        assert_eq!(pending_count(&app), 1, "scope {scope:?}");
        assert!(!blocked(&app, 0));
    }
}

#[test]
fn a_blocked_fill_retries_every_tick_and_warns_once_per_blockage() {
    let mut app = spawn_app_for(CONTACT, 1, &[1], Some(0.0));
    set_ramps(&mut app, true);
    for _ in 0..3 {
        app.update();
        assert_eq!(pending_count(&app), 0);
        assert!(blocked(&app, 0), "the blockage is remembered so it warns once");
    }
    set_ramps(&mut app, false);
    app.update();
    assert_eq!(pending_count(&app), 1, "the next tick with a clear spot fills");
    assert!(!blocked(&app, 0), "a successful spawn re-arms the warning");
    materialize_pending(&mut app);
    set_ramps(&mut app, true);
    destroy_one(&mut app);
    app.update();
    assert!(blocked(&app, 0), "a later blockage warns again");
}

#[test]
fn each_death_waits_its_own_full_delay() {
    let mut app = spawn_app(2, &[2], Some(1.0));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)));
    app.update();
    materialize_pending(&mut app);
    destroy_one(&mut app);
    for _ in 0..3 {
        app.update();
    }
    destroy_one(&mut app);
    app.update();
    let waits = refills(&app);
    assert_eq!(waits.len(), 2);
    assert!(
        waits[0].expect("first slot counts down") < waits[1].expect("second slot counts down"),
        "the second death does not join the first countdown: {waits:?}"
    );
    for _ in 0..10 {
        app.update();
        if pending_count(&app) == 1 {
            break;
        }
    }
    assert_eq!(pending_count(&app), 1, "the first slot refills alone");
    assert_eq!(refills(&app).len(), 1, "the second slot keeps waiting");
    for _ in 0..10 {
        app.update();
        if pending_count(&app) == 2 {
            break;
        }
    }
    assert_eq!(pending_count(&app), 2);
    assert!(refills(&app).is_empty());
}

#[test]
fn deaths_in_the_same_tick_each_wait_for_their_delay() {
    let mut app = spawn_app(3, &[3], Some(1000.0));
    app.update();
    materialize_pending(&mut app);
    destroy_one(&mut app);
    destroy_one(&mut app);
    app.update();
    assert_eq!(pending_count(&app), 0, "both vacancies wait for their delay");
    assert_eq!(refills(&app).len(), 2);
    expire_countdown(&mut app);
    app.update();
    assert_eq!(pending_count(&app), 2, "the expired delays refill both");
}

#[test]
fn expiring_selected_cooldowns_advances_pending_and_missing_slots() {
    let map_config = MapConfig {
        actor_spawn_zones: vec![
            ActorSpawnZone {
                count: vec![2],
                respawn_secs: Some(90.0),
                ..test_kinds::spawn_zone(CONTACT, [0, 1], [0, 1])
            },
            ActorSpawnZone {
                respawn_secs: Some(180.0),
                ..test_kinds::spawn_zone(BEAM, [0, 1], [0, 1])
            },
        ],
        ..MapConfig::for_grid(vec![floored_row(1)], geometry(1, 1))
    };
    let mut contact = pending_spawn(1, 60);
    contact.kind = CONTACT.to_owned();
    let mut beam = pending_spawn(2, 60);
    beam.zone_idx = 1;
    let mut pending = PendingActorSpawns(vec![contact, beam]);
    let mut spawner = ActorSpawner::default();
    spawner.refills.insert(0, vec![Some(60.0)]);
    spawner.refills.insert(1, vec![Some(120.0)]);

    let count = expedite_actor_respawns(
        &ActorMap::default(),
        &mut pending,
        &mut spawner,
        &map_config,
        1,
        100,
        Some(CONTACT),
    );

    assert_eq!(count, 2);
    assert_eq!(pending.0[0].due_tick, 100);
    assert_eq!(pending.0[1].due_tick, 60);
    assert_eq!(spawner.refills[&0], vec![Some(0.0)]);
    assert_eq!(spawner.refills[&1], vec![Some(120.0)]);
}

#[test]
fn a_zone_without_beam_in_spawns_its_actors_the_tick_their_slots_fill() {
    let mut app = spawn_app(2, &[2], None);
    app.world_mut().resource_mut::<MapConfig>().actor_spawn_zones[0].beam_in_secs = 0.0;
    app.update();
    assert_eq!(pending_count(&app), 0);
    assert_eq!(live_count(&app), 2);
}

#[test]
fn due_spawns_drain_in_queue_order_from_their_due_tick() {
    let mut pending = vec![pending_spawn(1, 3), pending_spawn(2, 15), pending_spawn(3, 6)];

    assert!(take_due_spawns(&mut pending, 2).is_empty());
    let due = take_due_spawns(&mut pending, 14);
    assert_eq!(
        due.iter().map(|spawn| spawn.actor_id).collect::<Vec<_>>(),
        vec![ActorId(1), ActorId(3)]
    );
    assert_eq!(pending.len(), 1);
    let due = take_due_spawns(&mut pending, 15);
    assert_eq!(due.len(), 1);
    assert!(pending.is_empty());
}

#[test]
fn a_switched_zone_spawns_nothing_until_its_switch_turns_on_then_fills_at_once() {
    let mut app = switched_app(Some(1000.0), 2);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(pending_count(&app), 0, "no initial fill");

    set_switch(&mut app, true);
    app.update();
    assert_eq!(
        pending_count(&app),
        2,
        "turning on fills on the next pass, whatever the respawn time"
    );
    app.update();
    assert_eq!(pending_count(&app), 2, "an active full zone queues nothing more");
}

#[test]
fn switching_off_and_on_does_not_restart_a_countdown() {
    let mut app = switched_app(Some(1000.0), 1);
    app.update();
    set_switch(&mut app, true);
    app.update();
    materialize_pending(&mut app);
    destroy_one(&mut app);
    app.update();
    assert!(matches!(refills(&app).as_slice(), [Some(secs)] if *secs > 999.0));
    set_switch(&mut app, false);
    app.update();
    assert!(
        matches!(refills(&app).as_slice(), [Some(secs)] if *secs > 999.0),
        "switching off keeps the countdown: {:?}",
        refills(&app)
    );
    expire_countdown(&mut app);
    app.update();
    assert_eq!(pending_count(&app), 0, "a due zone waits for its switch");
    set_switch(&mut app, true);
    app.update();
    assert_eq!(
        pending_count(&app),
        1,
        "the switch fills the due zone without a fresh countdown"
    );
    assert!(refills(&app).is_empty());
}

#[test]
fn a_kill_while_switched_off_arms_the_countdown() {
    let mut app = switched_app(Some(0.0), 1);
    app.update();
    set_switch(&mut app, true);
    app.update();
    materialize_pending(&mut app);
    set_switch(&mut app, false);
    app.update();
    destroy_one(&mut app);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(pending_count(&app), 0, "the vacancy waits for the switch");
    set_switch(&mut app, true);
    app.update();
    assert_eq!(pending_count(&app), 1, "turning on refills the vacancy at once");
}

#[test]
fn an_active_switched_zone_refills_kills_on_its_timer() {
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
fn a_reset_leaves_a_switched_off_zone_waiting_and_refills_an_active_one() {
    let mut app = switched_app(Some(0.0), 1);
    app.update();
    reset(&mut app, ActorRespawnScope::All);
    app.update();
    assert_eq!(pending_count(&app), 0);

    set_switch(&mut app, true);
    app.update();
    materialize_pending(&mut app);
    reset(&mut app, ActorRespawnScope::All);
    assert_eq!(live_count(&app), 0);
    app.update();
    assert_eq!(
        pending_count(&app),
        1,
        "an active zone refills after a reset like any other"
    );
}

#[test]
fn expediting_respawns_makes_a_switched_off_zone_due_for_its_switch() {
    let mut app = switched_app(Some(1000.0), 1);
    app.update();
    set_switch(&mut app, true);
    app.update();
    materialize_pending(&mut app);
    set_switch(&mut app, false);
    app.update();
    destroy_one(&mut app);
    app.update();
    let expedited = app
        .world_mut()
        .run_system_once(
            |actors: Res<ActorMap>,
             mut pending: ResMut<PendingActorSpawns>,
             mut spawner: ResMut<ActorSpawner>,
             map_config: Res<MapConfig>,
             tick: Res<ServerTick>| {
                expedite_actor_respawns(&actors, &mut pending, &mut spawner, &map_config, 1, tick.0, None)
            },
        )
        .expect("expedite system failed");
    assert_eq!(expedited, 1);
    app.update();
    assert_eq!(pending_count(&app), 0, "still waits for the switch");
    set_switch(&mut app, true);
    app.update();
    assert_eq!(pending_count(&app), 1);
}

#[test]
fn a_zone_that_starts_on_fills_at_boot_and_holds_once_its_switch_turns_on() {
    let mut app = switched_app(Some(0.0), 2);
    app.world_mut().resource_mut::<MapConfig>().actor_spawn_zones[0].initially_on = true;
    app.update();
    assert_eq!(pending_count(&app), 2);
    materialize_pending(&mut app);
    set_switch(&mut app, true);
    app.update();
    destroy_one(&mut app);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(pending_count(&app), 0, "a zone its switch holds back does not refill");
    set_switch(&mut app, false);
    app.update();
    assert_eq!(pending_count(&app), 1);
}

#[test]
fn a_zone_that_starts_on_but_boots_switched_waits_for_the_switch_to_turn_off() {
    let mut app = switched_app(Some(1000.0), 2);
    app.world_mut().resource_mut::<MapConfig>().actor_spawn_zones[0].initially_on = true;
    set_switch(&mut app, true);
    app.update();
    assert_eq!(pending_count(&app), 0);
    set_switch(&mut app, false);
    app.update();
    assert_eq!(pending_count(&app), 2);
}

#[test]
fn a_zone_without_a_respawn_time_never_refills_even_when_toggled() {
    let mut app = switched_app(None, 1);
    app.update();
    set_switch(&mut app, true);
    app.update();
    assert_eq!(pending_count(&app), 1);
    materialize_pending(&mut app);
    destroy_one(&mut app);
    set_switch(&mut app, false);
    app.update();
    set_switch(&mut app, true);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(pending_count(&app), 0);
    assert_eq!(refills(&app), vec![None], "the slot is lost for good");
}

#[test]
fn flying_spawns_reselect_blocked_reservations_and_restart_the_warning() {
    let mut app = spawn_app_for(BEAM, 2, &[1], None);
    make_beam_kind_fly(&mut app);
    app.world_mut().resource_mut::<MapConfig>().actor_spawn_zones[0].beam_in_secs = 1.0;
    for cell in &mut app.world_mut().resource_mut::<MapConfig>().grids[0].levels[0]
        .cells
        .rows[0]
    {
        cell.has_floor = false;
    }
    app.update();
    let (spawn_pos, due_tick, first_id) = {
        let spawn = &app.world().resource::<PendingActorSpawns>().0[0];
        (spawn.pos, spawn.due_tick, spawn.actor_id)
    };
    assert!(spawn_pos.y > 0.0);
    set_walls(
        &mut app,
        vec![Wall {
            x1: -10.0,
            z1: spawn_pos.z,
            x2: 10.0,
            z2: spawn_pos.z,
            y: -1.0,
            height: 10.0,
            width: 1.0,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
    );
    app.world_mut().resource_mut::<ServerTick>().0 = due_tick;
    app.update();
    assert_eq!(live_count(&app), 0);
    assert_eq!(pending_count(&app), 1);
    set_walls(&mut app, Vec::new());
    let replacement_due = {
        let pending = app.world().resource::<PendingActorSpawns>();
        let replacement = &pending.0[0];
        assert_ne!(replacement.actor_id, first_id);
        assert_ne!(replacement.pos, spawn_pos);
        assert_eq!(replacement.reserved_tick, due_tick);
        assert!(replacement.due_tick > due_tick);
        replacement.due_tick
    };
    app.world_mut().resource_mut::<ServerTick>().0 = replacement_due - 1;
    app.update();
    assert_eq!(live_count(&app), 0);
    app.world_mut().resource_mut::<ServerTick>().0 = replacement_due;
    app.update();
    let actors = app.world().resource::<ActorMap>();
    let actor = actors
        .values()
        .next()
        .expect("flying actor missing after obstruction cleared");
    assert!(actor.flight.is_some());
}

#[test]
fn joins_add_only_new_slots_without_refilling_deaths_or_skipping_cooldowns() {
    for respawn_secs in [None, Some(1000.0)] {
        let mut app = scaled_app(8, &[3, 5, 6], respawn_secs);
        app.update();
        materialize_pending(&mut app);
        destroy_one(&mut app);
        app.update();
        assert_eq!(live_count(&app), 2);
        assert_eq!(pending_count(&app), 0);
        add_player(&mut app, 2, true);
        app.update();
        assert_eq!(pending_count(&app), 2, "only the two added slots can spawn");
        if respawn_secs.is_some() {
            assert!(matches!(refills(&app).as_slice(), [Some(remaining)] if *remaining > 900.0));
        } else {
            assert_eq!(refills(&app), vec![None]);
        }
        materialize_pending(&mut app);
        assert_eq!(live_count(&app), 4);
        add_player(&mut app, 3, false);
        app.update();
        assert_eq!(pending_count(&app), 0, "connections awaiting login do not count");
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .begin_respawn(PlayerId(2), 10.0);
        app.update();
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .get_mut(&PlayerId(3))
            .expect("player")
            .connection
            .logged_in = true;
        app.update();
        assert_eq!(
            pending_count(&app),
            1,
            "a dead logged-in player still counts toward the third-player quota"
        );
        materialize_pending(&mut app);
        assert_eq!(live_count(&app), 5);
        if respawn_secs.is_some() {
            expire_countdown(&mut app);
            app.update();
            assert_eq!(
                pending_count(&app),
                1,
                "the original dead slot still refills when its timer expires"
            );
        }
    }
}

#[test]
fn departures_preserve_announced_spawns_and_let_surplus_actors_die_off() {
    let mut app = scaled_app(4, &[1, 3], Some(0.0));
    app.update();
    materialize_pending(&mut app);
    add_player(&mut app, 2, true);
    app.update();
    assert_eq!(pending_count(&app), 2);
    leave_player(&mut app, 2);
    app.update();
    assert_eq!(pending_count(&app), 2);
    materialize_pending(&mut app);
    assert_eq!(live_count(&app), 3);
    for remaining in [2, 1] {
        destroy_one(&mut app);
        app.update();
        assert_eq!(live_count(&app), remaining);
        assert_eq!(pending_count(&app), 0);
    }
    destroy_one(&mut app);
    app.update();
    assert_eq!(pending_count(&app), 1);
}

#[test]
fn rejoining_does_not_add_actors_while_the_zone_already_has_its_quota() {
    let mut app = scaled_app(6, &[1, 3], None);
    add_player(&mut app, 2, true);
    app.update();
    materialize_pending(&mut app);
    assert_eq!(live_count(&app), 3);
    leave_player(&mut app, 2);
    app.update();
    add_player(&mut app, 3, true);
    app.update();
    assert_eq!(pending_count(&app), 0);
    destroy_one(&mut app);
    app.update();
    assert_eq!(
        pending_count(&app),
        0,
        "unused additions cannot later refill a killed slot"
    );
}

#[test]
fn switched_off_join_slots_survive_until_enabled_and_shrink_when_players_leave() {
    let mut app = scaled_app(6, &[2, 4, 5], None);
    let zone = &mut app.world_mut().resource_mut::<MapConfig>().actor_spawn_zones[0];
    zone.switch = Some(GUARDS);
    zone.initially_on = false;
    set_switch(&mut app, true);
    app.update();
    materialize_pending(&mut app);
    destroy_one(&mut app);
    set_switch(&mut app, false);
    add_player(&mut app, 2, true);
    add_player(&mut app, 3, true);
    app.update();
    assert_eq!(pending_count(&app), 0);
    leave_player(&mut app, 3);
    app.update();
    set_switch(&mut app, true);
    app.update();
    assert_eq!(
        pending_count(&app),
        2,
        "the lost third-player slot and prior death stay empty"
    );
    materialize_pending(&mut app);
    assert_eq!(live_count(&app), 3);
}

#[test]
fn blocked_join_slots_are_discarded_on_departure_without_enabling_respawns() {
    let mut app = scaled_app(1, &[1, 3], None);
    app.update();
    materialize_pending(&mut app);
    add_player(&mut app, 2, true);
    app.update();
    assert_eq!(pending_count(&app), 0);
    assert!(blocked(&app, 0));
    leave_player(&mut app, 2);
    app.update();
    destroy_one(&mut app);
    app.update();
    assert_eq!(pending_count(&app), 0);
}

#[test]
fn a_rejoin_after_a_kill_neither_revives_a_permanent_loss_nor_skips_the_countdown() {
    for respawn_secs in [None, Some(1000.0)] {
        let mut app = scaled_app(6, &[1, 3], respawn_secs);
        add_player(&mut app, 2, true);
        app.update();
        materialize_pending(&mut app);
        destroy_one(&mut app);
        app.update();
        assert_eq!(live_count(&app), 2);
        leave_player(&mut app, 2);
        app.update();
        add_player(&mut app, 3, true);
        app.update();
        assert_eq!(pending_count(&app), 0, "the killed slot is not a join slot");
        if respawn_secs.is_some() {
            expire_countdown(&mut app);
            app.update();
            assert_eq!(pending_count(&app), 1, "the countdown still refills the kill");
        }
    }
}

#[test]
fn blocked_join_slots_fill_when_space_clears_without_refilling_other_dead_slots() {
    let mut app = scaled_app(2, &[2, 3], None);
    app.update();
    materialize_pending(&mut app);
    add_player(&mut app, 2, true);
    app.update();
    assert_eq!(pending_count(&app), 0);
    destroy_one(&mut app);
    app.update();
    assert_eq!(pending_count(&app), 1);
    materialize_pending(&mut app);
    app.update();
    assert_eq!(live_count(&app), 2);
    assert_eq!(pending_count(&app), 0);
}

#[test]
fn actor_resets_fill_to_the_current_player_count() {
    for scope in [ActorRespawnScope::Dead, ActorRespawnScope::All] {
        let mut app = scaled_app(5, &[2, 4], None);
        add_player(&mut app, 2, true);
        app.update();
        materialize_pending(&mut app);
        for _ in 0..3 {
            destroy_one(&mut app);
        }
        leave_player(&mut app, 2);
        reset(&mut app, scope);
        app.update();
        assert_eq!(live_count(&app) + pending_count(&app), 2);
        materialize_pending(&mut app);
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(live_count(&app), 2);
        assert_eq!(pending_count(&app), 0);
    }
}

#[test]
fn multiplayer_only_zones_stay_empty_without_two_logged_in_players() {
    let mut app = scaled_app(3, &[0, 2], None);
    leave_player(&mut app, 1);
    app.update();
    assert_eq!(pending_count(&app), 0);
    add_player(&mut app, 1, true);
    app.update();
    assert_eq!(pending_count(&app), 0);
    add_player(&mut app, 2, true);
    app.update();
    assert_eq!(pending_count(&app), 2);
    add_player(&mut app, 3, true);
    app.update();
    assert_eq!(pending_count(&app), 2, "the last entry caps larger sessions");
}

#[test]
fn blocked_flying_beam_ins_retry_only_their_reserved_slot() {
    let mut app = spawn_app_for(BEAM, 8, &[2], None);
    app.world_mut().resource_mut::<MapConfig>().actor_spawn_zones[0].count = vec![2, 3];
    make_beam_kind_fly(&mut app);
    add_player(&mut app, 1, true);
    app.update();
    materialize_pending(&mut app);
    destroy_one(&mut app);
    app.update();
    add_player(&mut app, 2, true);
    app.update();
    assert_eq!(pending_count(&app), 1);
    let due_tick = app.world().resource::<PendingActorSpawns>().0[0].due_tick;
    set_walls(
        &mut app,
        vec![Wall {
            x1: -100.0,
            x2: 100.0,
            z1: 0.0,
            z2: 0.0,
            y: -10.0,
            height: 100.0,
            width: 100.0,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
    );
    app.world_mut().resource_mut::<ServerTick>().0 = due_tick;
    app.update();
    assert_eq!(live_count(&app), 1);
    assert_eq!(pending_count(&app), 0);
    set_walls(&mut app, Vec::new());
    app.update();
    assert_eq!(
        pending_count(&app),
        1,
        "retry must not restore the previously killed actor"
    );
    materialize_pending(&mut app);
    assert_eq!(live_count(&app), 2);
}

#[test]
fn a_zone_stops_filling_once_any_player_has_reached_its_checkpoint() {
    let mut app = course_app(2, CheckpointResponse::Stop, Some(1000.0));
    reach(&mut app, PlayerId(1), 1);
    app.update();
    assert_eq!(pending_count(&app), 1, "checkpoint 1 leaves the encounter open");
    materialize_pending(&mut app);
    destroy_one(&mut app);
    app.update();
    assert!(matches!(refills(&app).as_slice(), [Some(secs)] if *secs > 999.0));

    reach(&mut app, PlayerId(2), 2);
    expire_countdown(&mut app);
    app.update();
    assert_eq!(pending_count(&app), 0, "the furthest player closes it for everyone");

    reach(&mut app, PlayerId(2), 1);
    app.update();
    assert_eq!(pending_count(&app), 1, "moving the course back reopens it");
}

#[test]
fn a_destroying_zone_drops_its_beam_ins_at_its_checkpoint() {
    let mut app = course_app(1, CheckpointResponse::Destroy, None);
    app.update();
    assert_eq!(pending_count(&app), 1);
    reach(&mut app, PlayerId(1), 1);
    app.update();
    assert_eq!(pending_count(&app), 0);

    let mut stopping = course_app(1, CheckpointResponse::Stop, None);
    stopping.update();
    reach(&mut stopping, PlayerId(1), 1);
    stopping.update();
    assert_eq!(
        pending_count(&stopping),
        1,
        "a stopping zone lets an announced beam-in materialize"
    );
}
