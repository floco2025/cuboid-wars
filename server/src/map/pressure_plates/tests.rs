use bevy::{ecs::system::RunSystemOnce, prelude::*};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{
    switches::PressureSwitches,
    system::{player_on_plate, presser_of_switch},
};
use crate::{
    actors::{ActorMap, ActorRespawnTimers, PendingActorSpawns},
    combat::{DeathSource, PendingExplosions, kill_player},
    config::{
        FireworksConfig, LightingMode, PlayerRespawnMode, QuestKind, RespawnConfig, ServerGameplayConfig, WeatherMode,
    },
    map::{
        CellGrid, EdgeGrid, LevelGrid, LightState, MapConfig, MapFireworks, PlayerSpawnZone, PressurePlateRuntime,
        WeatherState, map_plugin,
    },
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap, players_group_respawn_system, players_respawn_system},
    portals::PortalAssignments,
    quests::{
        QuestBoard, QuestCatalog,
        test_support::{catalog, completed, drain, feed_lines, quest},
    },
    schedule::{ServerSet, configure_server_schedule},
    test_geometry::{CELL, LEVEL_HEIGHT, geometry},
};
use common::{
    config::{DeathTrigger, PressureSwitchActivation, PressureSwitchConfig, SwitchHold},
    constants::FIREWORK_SHOW_SECS,
    map::{CarrierRun, Carriers, MapGeometry},
    physics::CollisionWorld,
    protocol::{
        Barrier, BarrierId, BarrierKindId, BridgeId, BridgeKindId, Carrier, CarrierId, HexColor, KindDef, LightBridge,
        MapLayout, MapSettings, PlateState, PlayerId, PlayerMarker, PortalMode, Position, QuestId, QuestScope,
        ServerMessage, ServerTick, SwitchDef, SwitchId, server_tick_advance_system,
    },
};

const LOBBY: BarrierId = BarrierId(0);
const SKYWAY: BridgeId = BridgeId(0);
// The harness switches, in catalog order: the lobby barriers', the skyway
// bridges', the fireworks', and one nothing names.
const LOBBY_SWITCH: SwitchId = SwitchId(0);
const SKYWAY_SWITCH: SwitchId = SwitchId(1);
const FIREWORKS_SWITCH: SwitchId = SwitchId(2);
const SPARE_SWITCH: SwitchId = SwitchId(3);
const FIREWORK_COOLDOWN_SECS: f32 = 1.0;

fn make_plate(level: u8, col: i32, row: i32) -> PressurePlateRuntime {
    PressurePlateRuntime {
        carrier: CarrierId::WORLD,
        level,
        col,
        row,
        switch: LOBBY_SWITCH,
    }
}

// Grid 1x1 centers the world origin on the cell at (0, 0), so the plate
// covers world-x in [-CELL/2, CELL/2] and the inner-50% rect is
// [-CELL/4, CELL/4] on each axis.
fn geom() -> MapGeometry {
    geometry(1, 1)
}

#[test]
fn dead_center_triggers() {
    let plate = make_plate(0, 0, 0);
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
    assert!(player_on_plate(&plate, &pos, &geom()));
}

#[test]
fn just_inside_inner_rect_triggers() {
    let plate = make_plate(0, 0, 0);
    // Inner rect goes from cell_x + 0.25*size to cell_x + 0.75*size.
    // cell_x for col=0 on a 1x1 grid is -size/2. So inner-rect minimum x
    // is -size/2 + 0.25*size = -0.25 * size. Sample just inside.
    let just_inside = -0.25 * CELL + 0.01;
    let pos = Position {
        x: just_inside,
        y: 0.0,
        z: just_inside,
    };
    assert!(player_on_plate(&plate, &pos, &geom()));
}

#[test]
fn just_outside_inner_rect_does_not_trigger() {
    let plate = make_plate(0, 0, 0);
    // Just outside the inner-50% rect on x; z still centered.
    let outside_x = -0.25 * CELL - 0.01;
    let pos = Position {
        x: outside_x,
        y: 0.0,
        z: 0.0,
    };
    assert!(!player_on_plate(&plate, &pos, &geom()));
}

#[test]
fn corner_of_cell_does_not_trigger() {
    let plate = make_plate(0, 0, 0);
    // Cell corner sits at +/- size/2 on both axes — well outside the
    // inner-50% rect.
    let pos = Position {
        x: CELL / 2.0,
        y: 0.0,
        z: CELL / 2.0,
    };
    assert!(!player_on_plate(&plate, &pos, &geom()));
}

#[test]
fn level_above_does_not_trigger() {
    let plate = make_plate(0, 0, 0);
    let pos = Position {
        x: 0.0,
        y: LEVEL_HEIGHT,
        z: 0.0,
    };
    assert!(!player_on_plate(&plate, &pos, &geom()));
}

#[test]
fn small_y_offset_within_level_still_triggers() {
    let plate = make_plate(0, 0, 0);
    let pos = Position {
        x: 0.0,
        y: LEVEL_HEIGHT / 2.0 - 0.01,
        z: 0.0,
    };
    assert!(player_on_plate(&plate, &pos, &geom()));
}

#[test]
fn non_zero_level_plate_triggers_at_matching_y() {
    let plate = make_plate(2, 0, 0);
    let pos = Position {
        x: 0.0,
        y: 2.0 * LEVEL_HEIGHT,
        z: 0.0,
    };
    assert!(player_on_plate(&plate, &pos, &geom()));
}

#[test]
fn presser_prefers_a_fresh_press_over_a_standing_holder() {
    let plates = vec![
        make_plate(0, 0, 0),
        make_plate(0, 1, 0),
        PressurePlateRuntime {
            switch: SKYWAY_SWITCH,
            ..make_plate(0, 2, 0)
        },
    ];
    let holders = HashMap::from([(0, PlayerId(1)), (1, PlayerId(2)), (2, PlayerId(3))]);
    let prev_held = HashSet::from([0]);

    assert_eq!(
        presser_of_switch(LOBBY_SWITCH, &holders, &prev_held, &plates),
        Some(PlayerId(2))
    );
    assert_eq!(
        presser_of_switch(SKYWAY_SWITCH, &holders, &prev_held, &plates),
        Some(PlayerId(3))
    );
    assert_eq!(presser_of_switch(FIREWORKS_SWITCH, &holders, &prev_held, &plates), None);
}

fn firework_plate() -> PressurePlateRuntime {
    PressurePlateRuntime {
        carrier: CarrierId::WORLD,
        level: 0,
        col: 0,
        row: 0,
        switch: FIREWORKS_SWITCH,
    }
}

fn lobby_plate() -> PressurePlateRuntime {
    PressurePlateRuntime {
        switch: LOBBY_SWITCH,
        ..firework_plate()
    }
}

fn skyway_plate() -> PressurePlateRuntime {
    PressurePlateRuntime {
        switch: SKYWAY_SWITCH,
        ..firework_plate()
    }
}

fn spare_plate() -> PressurePlateRuntime {
    PressurePlateRuntime {
        switch: SPARE_SWITCH,
        ..firework_plate()
    }
}

fn switch_def(id: &str, policy: PressureSwitchConfig) -> SwitchDef {
    SwitchDef {
        id: id.to_owned(),
        plate_color: None,
        policy,
    }
}

fn harness_settings(config: &ServerGameplayConfig) -> MapSettings {
    let mut settings = config.maps[&config.default_map].settings.clone();
    settings.switches = vec![
        switch_def("lobby", PressureSwitchConfig::default()),
        switch_def("skyway", PressureSwitchConfig::default()),
        switch_def(
            "fireworks",
            PressureSwitchConfig {
                activation: PressureSwitchActivation::Momentary,
                reset_on_player_death: DeathTrigger::Never,
                held: SwitchHold::Everyone,
            },
        ),
        switch_def("spare", PressureSwitchConfig::default()),
    ];
    settings.barrier_kinds = vec![kind("lobby", "lobby")];
    settings.bridge_kinds = vec![kind("skyway", "skyway")];
    settings
}

fn app(config: ServerGameplayConfig, plates: Vec<PressurePlateRuntime>) -> App {
    app_with_layout(config, plates, MapLayout::default())
}

fn app_with_layout(config: ServerGameplayConfig, plates: Vec<PressurePlateRuntime>, mut layout: MapLayout) -> App {
    if layout.barriers.is_empty() {
        layout.barriers.push(Barrier {
            id: LOBBY,
            kind: BarrierKindId(0),
            switch: Some(LOBBY_SWITCH),
            switch_inverted: false,
            x1: 100.0,
            z1: 100.0,
            x2: 102.0,
            z2: 100.0,
            width: 0.1,
            y: 0.0,
            height: 2.0,
            level: 0,
            levels: 1,
            carrier: CarrierId::WORLD,
        });
    }
    if layout.light_bridges.is_empty() {
        layout.light_bridges.push(LightBridge {
            id: SKYWAY,
            kind: BridgeKindId(0),
            switch: Some(SKYWAY_SWITCH),
            switch_inverted: false,
            x1: 100.0,
            z1: 100.0,
            x2: 102.0,
            z2: 102.0,
            y: 0.0,
            thickness: 0.1,
            level: 0,
            carrier: CarrierId::WORLD,
        });
    }
    let quest_catalog = QuestCatalog::from_config(&config);
    let board = QuestBoard::from_catalog(&quest_catalog, Some(FIREWORKS_SWITCH));
    let settings = harness_settings(&config);
    let (barrier_table, bridge_table, switch_table) = settings.kind_tables().expect("harness settings rejected");
    let mut app = App::new();
    app.insert_resource(WeatherState::new(config.cycles.weather.clone(), WeatherMode::Clear))
        .insert_resource(LightState::new(config.cycles.lighting.clone(), LightingMode::Bright))
        .insert_resource(settings)
        .insert_resource(CollisionWorld::from_map_layout(&layout, &Default::default()))
        .insert_resource(Carriers::from_layout(&layout))
        .insert_resource(layout);
    app.add_plugins(MinimalPlugins)
        .insert_resource(MapConfig {
            pressure_plates: plates,
            ..MapConfig::for_grid(
                vec![LevelGrid {
                    cells: CellGrid::new(2, 2),
                    edges: EdgeGrid::new(2, 2),
                    barrier_edges: EdgeGrid::new(2, 2),
                }],
                geometry(2, 2),
            )
        })
        .insert_resource(geometry(2, 2))
        .insert_resource(PlayerMap::default())
        .insert_resource(config)
        .insert_resource(quest_catalog)
        .insert_resource(board)
        .insert_resource(barrier_table)
        .insert_resource(bridge_table)
        .insert_resource(switch_table)
        .insert_resource(MapFireworks(Some(FireworksConfig {
            switch_inverted: false,

            switch: "fireworks".to_owned(),
            cooldown_secs: FIREWORK_COOLDOWN_SECS,
        })))
        .insert_resource(PlateState::default())
        .init_resource::<ServerTick>()
        .add_plugins(map_plugin);
    configure_server_schedule(&mut app);
    app.add_systems(Update, server_tick_advance_system.in_set(ServerSet::Prepare));
    app.add_systems(Update, players_group_respawn_system.in_set(ServerSet::Lifecycle));
    app
}

fn kind(id: &str, _switch: &str) -> KindDef {
    KindDef {
        id: id.to_owned(),
        color: HexColor([0; 3]),
    }
}

// A logged-in player standing in the middle of cell (0, 0).
fn standing_player(app: &mut App, id: u32) -> (Entity, UnboundedReceiver<ServerToClient>) {
    let geometry = *app.world().resource::<MapGeometry>();
    let pos = Position {
        x: geometry.cell_center_x(0),
        y: 0.0,
        z: geometry.cell_center_z(0),
    };
    let entity = app.world_mut().spawn((PlayerMarker, PlayerId(id), pos)).id();
    let (tx, mut rx) = unbounded_channel();
    let mut info = PlayerInfo::new(entity, tx);
    info.connection.logged_in = true;
    while rx.try_recv().is_ok() {}
    app.world_mut().resource_mut::<PlayerMap>().insert(PlayerId(id), info);
    (entity, rx)
}

fn leave(app: &mut App, id: u32, entity: Entity) {
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .disconnect(&PlayerId(id), 2.0);
    app.world_mut().despawn(entity);
}

fn step_off(app: &mut App, entity: Entity) {
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<Position>()
        .expect("position")
        .x += 100.0;
}

fn step_on(app: &mut App, entity: Entity) {
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<Position>()
        .expect("position")
        .x -= 100.0;
}

// Moves the player onto the plate one cell east of the harness plate.
fn step_onto_second_plate(app: &mut App, entity: Entity) {
    let cell = app.world().resource::<MapGeometry>().cell_size();
    let mut pos = *app.world().get::<Position>(entity).expect("player position missing");
    pos.x = app.world().resource::<MapGeometry>().cell_center_x(0) + cell;
    *app.world_mut()
        .get_mut::<Position>(entity)
        .expect("player position missing") = pos;
}

fn open_kinds(app: &App) -> Vec<BarrierId> {
    app.world().resource::<PlateState>().open_barriers.clone()
}

fn powered_kinds(app: &App) -> Vec<BridgeId> {
    app.world().resource::<PlateState>().powered_bridges.clone()
}

fn active_switches(app: &App) -> Vec<SwitchId> {
    app.world().resource::<PlateState>().active_switches.clone()
}

fn carrier_run(app: &App) -> CarrierRun {
    app.world()
        .resource::<PlateState>()
        .carrier_run(CarrierId(1))
        .expect("switched carrier missing from the plate state")
}

fn tick(app: &App) -> u32 {
    app.world().resource::<ServerTick>().0
}

fn skip_ticks(app: &mut App, ticks: u32) {
    app.world_mut().resource_mut::<ServerTick>().0 += ticks;
}

fn switch_lines(messages: &[ServerMessage], switch: &str) -> Vec<String> {
    feed_lines(messages)
        .into_iter()
        .filter(|line| line.contains(&format!("{switch} switch")))
        .collect()
}

fn shows(messages: &[ServerMessage]) -> usize {
    messages
        .iter()
        .filter(|msg| matches!(msg, ServerMessage::Firework(_)))
        .count()
}

fn clicks(messages: &[ServerMessage]) -> usize {
    messages
        .iter()
        .filter(|msg| matches!(msg, ServerMessage::PressurePlate(_)))
        .count()
}

fn firework_interval_ticks() -> u32 {
    ((FIREWORK_SHOW_SECS + FIREWORK_COOLDOWN_SECS) * 30.0).round() as u32
}

#[test]
fn a_locked_plate_neither_clicks_nor_fires() {
    let config = catalog(vec![
        quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None),
        quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, Some("gold")),
    ]);
    let mut app = app(config, vec![firework_plate()]);
    let (_, mut rx) = standing_player(&mut app, 1);

    app.update();
    app.update();

    let messages = drain(&mut rx);
    assert_eq!((clicks(&messages), shows(&messages)), (0, 0));
    assert!(
        !app.world()
            .resource::<QuestBoard>()
            .is_completed(&QuestId("show".to_owned()))
    );
}

#[test]
fn the_fireworks_switch_shows_once_per_interval_while_held() {
    let config = catalog(vec![quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, None)]);
    let mut app = app(config, vec![firework_plate()]);
    let (entity, mut rx) = standing_player(&mut app, 1);

    app.update();
    app.update();
    let messages = drain(&mut rx);
    assert_eq!(
        (clicks(&messages), shows(&messages)),
        (1, 1),
        "press + one show, not one per tick"
    );
    assert!(
        app.world()
            .resource::<QuestBoard>()
            .is_completed(&QuestId("show".to_owned()))
    );

    // A re-press during the show and cooldown waits it out.
    step_off(&mut app, entity);
    app.update();
    assert_eq!(clicks(&drain(&mut rx)), 1, "release click");
    step_on(&mut app, entity);
    app.update();
    assert_eq!(shows(&drain(&mut rx)), 0);

    // Held through the interval: the show repeats, the latched quest doesn't.
    skip_ticks(&mut app, firework_interval_ticks());
    app.update();
    let messages = drain(&mut rx);
    assert_eq!(shows(&messages), 1);
    assert!(!completed(&messages, "show"));

    // Released through the next interval: nothing.
    step_off(&mut app, entity);
    skip_ticks(&mut app, firework_interval_ticks());
    app.update();
    app.update();
    assert_eq!(shows(&drain(&mut rx)), 0);
}

#[test]
fn the_fireworks_switch_holds_only_with_everyone_on_its_plates() {
    let mut app = app(
        catalog(vec![]),
        vec![
            firework_plate(),
            PressurePlateRuntime {
                col: 1,
                ..firework_plate()
            },
        ],
    );
    let (first, mut rx) = standing_player(&mut app, 1);
    let (second, _) = standing_player(&mut app, 2);
    step_off(&mut app, second);
    app.update();
    assert_eq!(shows(&drain(&mut rx)), 0, "one of two players is not everyone");
    assert!(!active_switches(&app).contains(&FIREWORKS_SWITCH));

    step_on(&mut app, second);
    step_onto_second_plate(&mut app, second);
    app.update();
    assert_eq!(shows(&drain(&mut rx)), 1);
    assert!(active_switches(&app).contains(&FIREWORKS_SWITCH));
    step_off(&mut app, first);
    app.update();
    assert!(!active_switches(&app).contains(&FIREWORKS_SWITCH));
}

#[test]
fn an_everyone_toggle_flips_on_the_thresholds_rising_edge() {
    let mut app = app(
        catalog(vec![]),
        vec![
            lobby_plate(),
            PressurePlateRuntime {
                col: 1,
                ..lobby_plate()
            },
        ],
    );
    install_switches(
        &mut app,
        PressureSwitchConfig {
            activation: PressureSwitchActivation::Toggle,
            reset_on_player_death: DeathTrigger::Never,
            held: SwitchHold::Everyone,
        },
        PressureSwitchConfig::default(),
    );
    let (first, _) = standing_player(&mut app, 1);
    let (second, _) = standing_player(&mut app, 2);
    step_off(&mut app, second);
    app.update();
    assert!(open_kinds(&app).is_empty(), "one press of two is no threshold");

    step_on(&mut app, second);
    step_onto_second_plate(&mut app, second);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY]);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY], "holding the threshold flips once");

    step_off(&mut app, first);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY], "a toggle keeps its state on release");
    step_on(&mut app, first);
    app.update();
    assert!(open_kinds(&app).is_empty(), "meeting the threshold again flips back");
}

#[test]
fn an_everyone_toggle_flips_when_a_death_lowers_its_threshold() {
    for trigger in [DeathTrigger::Never, DeathTrigger::Any] {
        let mut app = app(
            catalog(vec![]),
            vec![
                lobby_plate(),
                PressurePlateRuntime {
                    col: 1,
                    ..lobby_plate()
                },
            ],
        );
        install_switches(
            &mut app,
            PressureSwitchConfig {
                activation: PressureSwitchActivation::Toggle,
                reset_on_player_death: trigger,
                held: SwitchHold::Everyone,
            },
            PressureSwitchConfig::default(),
        );
        let (holder, _) = standing_player(&mut app, 1);
        let (other, _) = standing_player(&mut app, 2);
        step_off(&mut app, other);
        app.update();
        assert!(open_kinds(&app).is_empty(), "one holder of two is no threshold");

        // The death leaves one living player, whom the held plate satisfies.
        die(&mut app, 2);
        app.update();
        app.update();
        if trigger == DeathTrigger::Never {
            assert_eq!(
                open_kinds(&app),
                [LOBBY],
                "the lowered threshold is a fresh rising edge"
            );
        } else {
            assert!(open_kinds(&app).is_empty(), "a reset switch needs a release first");
            step_off(&mut app, holder);
            app.update();
            step_on(&mut app, holder);
            app.update();
            assert_eq!(open_kinds(&app), [LOBBY]);
        }
    }
}

#[test]
fn a_lone_player_toggles_a_barrier_kind_with_each_press() {
    let mut app = app(catalog(Vec::new()), vec![lobby_plate()]);
    let (entity, mut rx) = standing_player(&mut app, 1);
    step_off(&mut app, entity);
    app.update();
    assert!(open_kinds(&app).is_empty());

    step_on(&mut app, entity);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY]);
    let lines = switch_lines(&drain(&mut rx), "lobby");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("turned on the lobby switch"), "{lines:?}");

    step_off(&mut app, entity);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY], "stepping off leaves the switch alone");
    assert!(switch_lines(&drain(&mut rx), "lobby").is_empty());

    step_on(&mut app, entity);
    app.update();
    assert!(open_kinds(&app).is_empty());
    assert_eq!(switch_lines(&drain(&mut rx), "lobby"), ["The lobby switch turned off"]);
}

#[test]
fn a_first_login_prints_no_off_lines() {
    let mut app = app(catalog(Vec::new()), vec![lobby_plate()]);
    app.update();
    assert!(open_kinds(&app).is_empty(), "empty plates stay off on an empty server");

    let (entity, mut rx) = standing_player(&mut app, 1);
    step_off(&mut app, entity);
    app.update();
    assert!(open_kinds(&app).is_empty());
    assert!(switch_lines(&drain(&mut rx), "lobby").is_empty());
}

#[test]
fn a_second_login_restores_hold_to_open() {
    let mut app = app(catalog(Vec::new()), vec![lobby_plate()]);
    let (entity, _rx) = standing_player(&mut app, 1);
    app.update();
    step_off(&mut app, entity);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY], "switched open");

    let (partner, _partner_rx) = standing_player(&mut app, 2);
    step_off(&mut app, partner);
    app.update();
    assert!(open_kinds(&app).is_empty(), "two players: open only while held");

    step_on(&mut app, entity);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY]);
    step_off(&mut app, entity);
    app.update();
    assert!(open_kinds(&app).is_empty());
}

#[test]
fn a_bridge_plate_powers_only_its_own_kind_and_says_so() {
    let mut app = app(catalog(Vec::new()), vec![skyway_plate(), lobby_plate()]);
    let (entity, mut rx) = standing_player(&mut app, 1);
    let (partner, _partner_rx) = standing_player(&mut app, 2);
    step_off(&mut app, entity);
    step_off(&mut app, partner);
    app.update();
    assert!(powered_kinds(&app).is_empty());
    drain(&mut rx);

    step_on(&mut app, entity);
    app.update();
    assert_eq!(powered_kinds(&app), [SKYWAY], "the held plate powers its bridges");
    assert_eq!(
        open_kinds(&app),
        [LOBBY],
        "and the barrier plate on the same cell opens its kind"
    );
    let lines = switch_lines(&drain(&mut rx), "skyway");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("turned on the skyway switch"), "{lines:?}");

    step_off(&mut app, entity);
    app.update();
    assert!(powered_kinds(&app).is_empty());
    assert_eq!(
        switch_lines(&drain(&mut rx), "skyway"),
        ["The skyway switch turned off"]
    );
}

#[test]
fn a_barrier_plate_never_powers_a_bridge_kind() {
    let mut app = app(catalog(Vec::new()), vec![lobby_plate()]);
    let (entity, _rx) = standing_player(&mut app, 1);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY]);
    assert!(powered_kinds(&app).is_empty());
    step_off(&mut app, entity);
    app.update();
    assert!(powered_kinds(&app).is_empty());
}

#[test]
fn one_switch_opens_a_barrier_kind_and_powers_a_bridge_kind_together() {
    let mut app = app(catalog(Vec::new()), vec![lobby_plate()]);
    app.world_mut().resource_mut::<MapLayout>().light_bridges[0].switch = Some(LOBBY_SWITCH);
    let switches = PressureSwitches::from_world(app.world_mut());
    app.insert_resource(switches);

    let (entity, _rx) = standing_player(&mut app, 1);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY]);
    assert_eq!(powered_kinds(&app), [SKYWAY]);
    assert_eq!(active_switches(&app), [LOBBY_SWITCH]);
    step_off(&mut app, entity);
    app.update();
    step_on(&mut app, entity);
    app.update();
    assert!(open_kinds(&app).is_empty());
    assert!(powered_kinds(&app).is_empty());
}

#[test]
fn a_plate_whose_switch_has_no_targets_still_clicks_and_feeds() {
    let mut app = app(catalog(Vec::new()), vec![spare_plate()]);
    let (_, mut rx) = standing_player(&mut app, 1);
    app.update();
    let messages = drain(&mut rx);
    assert_eq!(clicks(&messages), 1);
    let lines = switch_lines(&messages, "spare");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("turned on the spare switch"), "{lines:?}");
    assert_eq!(active_switches(&app), [SPARE_SWITCH]);
    assert!(open_kinds(&app).is_empty());
    assert!(powered_kinds(&app).is_empty());
}

fn switched_carrier_layout() -> MapLayout {
    MapLayout {
        carriers: vec![Carrier {
            switch_inverted: false,

            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: Position::default(),
            to: Position { x: 4.0, y: 0.0, z: 0.0 },
            travel_ticks: 60,
            pause_ticks: 30,
            phase_ticks: 0,
            switch: Some(LOBBY_SWITCH),
        }],
        ..Default::default()
    }
}

#[test]
fn a_switch_starts_and_stops_its_carrier_run() {
    let mut app = app_with_layout(catalog(Vec::new()), vec![lobby_plate()], switched_carrier_layout());
    configure_switches(&mut app, PressureSwitchActivation::Momentary, DeathTrigger::Never);
    app.update();
    assert_eq!(carrier_run(&app), CarrierRun::STOPPED);

    let (entity, _rx) = standing_player(&mut app, 1);
    app.update();
    let started = tick(&app);
    assert_eq!(
        carrier_run(&app),
        CarrierRun {
            running: true,
            run_ticks: 0,
            since_tick: started,
        }
    );
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(
        carrier_run(&app).since_tick,
        started,
        "holding the plate never re-stamps the run"
    );

    step_off(&mut app, entity);
    app.update();
    assert_eq!(
        carrier_run(&app),
        CarrierRun {
            running: false,
            run_ticks: 6,
            since_tick: started + 6,
        }
    );

    step_on(&mut app, entity);
    app.update();
    let resumed = carrier_run(&app);
    assert_eq!(
        (resumed.running, resumed.run_ticks, resumed.since_tick),
        (true, 6, started + 7)
    );
}

#[test]
fn a_death_reset_stops_the_carrier() {
    let mut app = app_with_layout(catalog(Vec::new()), vec![lobby_plate()], switched_carrier_layout());
    configure_switches(&mut app, PressureSwitchActivation::Toggle, DeathTrigger::Any);
    let (entity, _) = standing_player(&mut app, 1);
    standing_player(&mut app, 2);
    app.update();
    step_off(&mut app, entity);
    app.update();
    app.update();
    assert!(carrier_run(&app).running);
    die(&mut app, 1);
    app.update();
    let run = carrier_run(&app);
    assert!(!run.running);
    assert_eq!(run.run_ticks, 3);
    assert_eq!(run.since_tick, tick(&app));
}

#[test]
fn a_lone_player_toggles_a_bridge_kind_with_each_press() {
    let mut app = app(catalog(Vec::new()), vec![skyway_plate()]);
    let (entity, _rx) = standing_player(&mut app, 1);
    step_off(&mut app, entity);
    app.update();
    assert!(powered_kinds(&app).is_empty());

    step_on(&mut app, entity);
    app.update();
    assert_eq!(powered_kinds(&app), [SKYWAY]);

    step_off(&mut app, entity);
    app.update();
    assert_eq!(powered_kinds(&app), [SKYWAY], "stepping off leaves the switch alone");

    step_on(&mut app, entity);
    app.update();
    assert!(powered_kinds(&app).is_empty());
}

#[test]
fn solo_switches_start_from_the_plates_held_when_the_partner_leaves() {
    let mut app = app(catalog(Vec::new()), vec![lobby_plate()]);
    let (entity, mut rx) = standing_player(&mut app, 1);
    let (partner, _partner_rx) = standing_player(&mut app, 2);
    step_off(&mut app, partner);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY], "held open under the hold rule");
    drain(&mut rx);

    leave(&mut app, 2, partner);
    app.update();
    step_off(&mut app, entity);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY], "the held plate seeds the switch");
    assert!(switch_lines(&drain(&mut rx), "lobby").is_empty());
}

fn configure_switches(app: &mut App, activation: PressureSwitchActivation, trigger: DeathTrigger) {
    let switch = PressureSwitchConfig {
        activation,
        reset_on_player_death: trigger,
        held: SwitchHold::Any,
    };
    install_switches(app, switch, switch);
}

// Rebuilds the switches with the lobby and skyway policies.
fn install_switches(app: &mut App, lobby: PressureSwitchConfig, skyway: PressureSwitchConfig) {
    let mut settings = app.world().resource::<MapSettings>().clone();
    settings.switches[usize::from(LOBBY_SWITCH.0)].policy = lobby;
    settings.switches[usize::from(SKYWAY_SWITCH.0)].policy = skyway;
    app.insert_resource(settings);
    let switches = PressureSwitches::from_world(app.world_mut());
    app.insert_resource(switches);
}

fn assert_switches(app: &App, active: bool) {
    assert_eq!(!open_kinds(app).is_empty(), active, "barrier state");
    assert_eq!(!powered_kinds(app).is_empty(), active, "bridge state");
}

fn die(app: &mut App, id: u32) {
    assert!(
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .begin_respawn(PlayerId(id), 2.0)
    );
}

#[test]
fn momentary_needs_any_matching_plate_and_never_opens_for_missing_holders() {
    for activation in [PressureSwitchActivation::Momentary, PressureSwitchActivation::Auto] {
        for count in [1, 2, 4] {
            if activation == PressureSwitchActivation::Auto && count == 1 {
                continue;
            }
            let mut app = app(
                catalog(vec![]),
                vec![
                    lobby_plate(),
                    skyway_plate(),
                    PressurePlateRuntime {
                        col: 1,
                        ..lobby_plate()
                    },
                    PressurePlateRuntime {
                        col: 1,
                        ..skyway_plate()
                    },
                ],
            );
            configure_switches(&mut app, activation, DeathTrigger::Never);
            app.update();
            assert_switches(&app, false);
            let (holder, _) = standing_player(&mut app, 1);
            for id in 2..=count {
                let (entity, _) = standing_player(&mut app, id);
                step_off(&mut app, entity);
            }
            app.update();
            assert_switches(&app, true);
            step_off(&mut app, holder);
            app.update();
            assert_switches(&app, false);
            step_on(&mut app, holder);
            app.update();
            assert_switches(&app, true);
            die(&mut app, 1);
            app.update();
            assert_switches(&app, false);
            for id in 2..=count {
                die(&mut app, id);
                app.update();
                assert_switches(&app, false);
            }
        }
    }
}

#[test]
fn explicit_toggles_persist_through_joins_disconnects_and_an_empty_server() {
    let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
    configure_switches(&mut app, PressureSwitchActivation::Toggle, DeathTrigger::Never);
    let (first, _) = standing_player(&mut app, 1);
    app.update();
    step_off(&mut app, first);
    let (second, _) = standing_player(&mut app, 2);
    step_off(&mut app, second);
    app.update();
    assert_switches(&app, true);
    leave(&mut app, 1, first);
    app.update();
    assert_switches(&app, true);
    leave(&mut app, 2, second);
    app.update();
    assert_switches(&app, true);
    let (third, _) = standing_player(&mut app, 3);
    step_off(&mut app, third);
    app.update();
    assert_switches(&app, true);
    step_on(&mut app, third);
    app.update();
    assert_switches(&app, false);
}

#[test]
fn two_players_on_one_plate_produce_one_toggle_until_everyone_releases_it() {
    let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
    configure_switches(&mut app, PressureSwitchActivation::Toggle, DeathTrigger::Never);
    let (first, _) = standing_player(&mut app, 1);
    app.update();
    let (second, _) = standing_player(&mut app, 2);
    app.update();
    assert_switches(&app, true);
    step_off(&mut app, first);
    app.update();
    assert_switches(&app, true);
    step_off(&mut app, second);
    app.update();
    assert_switches(&app, true);
    step_on(&mut app, second);
    app.update();
    assert_switches(&app, false);
}

#[test]
fn toggle_death_policies_distinguish_solo_any_and_all_across_ticks() {
    for trigger in [
        DeathTrigger::Never,
        DeathTrigger::Solo,
        DeathTrigger::Any,
        DeathTrigger::All,
    ] {
        for count in [1, 2] {
            let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
            configure_switches(&mut app, PressureSwitchActivation::Toggle, trigger);
            for id in 1..=count {
                standing_player(&mut app, id);
            }
            app.update();
            assert_switches(&app, true);
            die(&mut app, 1);
            app.update();
            let reset = trigger == DeathTrigger::Any || (count == 1 && trigger != DeathTrigger::Never);
            assert_switches(&app, !reset);
            if count == 2 {
                app.update();
                assert_switches(&app, !reset);
                die(&mut app, 2);
                app.update();
                assert_switches(&app, matches!(trigger, DeathTrigger::Never | DeathTrigger::Solo));
            }
        }
    }
}

#[test]
fn group_death_resets_all_switches_and_momentary_holders_before_snapshot() {
    for activation in [PressureSwitchActivation::Toggle, PressureSwitchActivation::Momentary] {
        let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
        configure_switches(&mut app, activation, DeathTrigger::All);
        app.insert_resource(PlayerMap::new(RespawnConfig {
            players: PlayerRespawnMode::Group,
            ..default()
        }));
        standing_player(&mut app, 1);
        standing_player(&mut app, 2);
        app.update();
        assert_switches(&app, true);
        app.add_systems(
            Update,
            (|mut players: ResMut<PlayerMap>| {
                players.begin_respawn(PlayerId(1), 2.0);
            })
            .in_set(ServerSet::CombatDamage),
        );
        app.update();
        assert_switches(&app, false);
        assert!(app.world().resource::<PlayerMap>().values().all(PlayerInfo::is_dead));
    }
}

#[test]
fn death_reset_beats_a_press_after_movement_and_requires_a_fresh_press() {
    let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
    configure_switches(&mut app, PressureSwitchActivation::Toggle, DeathTrigger::Any);
    let (first, _) = standing_player(&mut app, 1);
    let (survivor, _) = standing_player(&mut app, 2);
    step_off(&mut app, survivor);
    app.update();
    step_off(&mut app, first);
    app.update();
    assert_switches(&app, true);
    app.add_systems(
        Update,
        (move |mut positions: Query<&mut Position>, mut once: Local<bool>, mut players: ResMut<PlayerMap>| {
            if !*once {
                positions.get_mut(survivor).expect("survivor position missing").x -= 100.0;
                players.begin_respawn(PlayerId(1), 2.0);
                *once = true;
            }
        })
        .in_set(ServerSet::CombatDamage),
    );
    app.update();
    assert_switches(&app, false);
    app.update();
    assert_switches(&app, false);
    step_off(&mut app, survivor);
    app.update();
    step_on(&mut app, survivor);
    app.update();
    assert_switches(&app, true);
}

#[test]
fn auto_toggles_reset_on_solo_death_but_explicit_momentary_ignores_reset_policy() {
    let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
    configure_switches(&mut app, PressureSwitchActivation::Auto, DeathTrigger::Solo);
    let (entity, _) = standing_player(&mut app, 1);
    app.update();
    step_off(&mut app, entity);
    app.update();
    assert_switches(&app, true);
    die(&mut app, 1);
    app.update();
    assert_switches(&app, false);

    let mut app = self::app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
    configure_switches(&mut app, PressureSwitchActivation::Momentary, DeathTrigger::Any);
    standing_player(&mut app, 1);
    standing_player(&mut app, 2);
    app.update();
    die(&mut app, 1);
    app.update();
    assert_switches(&app, true);
}

#[test]
fn all_reset_follows_the_last_survivor_departure_and_solo_uses_counts_at_the_event() {
    for trigger in [DeathTrigger::All, DeathTrigger::Solo] {
        let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
        configure_switches(&mut app, PressureSwitchActivation::Toggle, trigger);
        standing_player(&mut app, 1);
        let (partner, _) = standing_player(&mut app, 2);
        app.update();
        die(&mut app, 1);
        leave(&mut app, 2, partner);
        app.update();
        assert_switches(&app, trigger == DeathTrigger::Solo);
    }
    let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
    configure_switches(&mut app, PressureSwitchActivation::Toggle, DeathTrigger::Solo);
    standing_player(&mut app, 1);
    app.update();
    die(&mut app, 1);
    standing_player(&mut app, 2);
    app.update();
    assert_switches(&app, false);
}

#[test]
fn toggle_logout_policies_distinguish_solo_any_and_all() {
    for trigger in [
        DeathTrigger::Never,
        DeathTrigger::Solo,
        DeathTrigger::Any,
        DeathTrigger::All,
    ] {
        for count in [1, 2] {
            let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
            configure_switches(&mut app, PressureSwitchActivation::Toggle, trigger);
            let (first, _) = standing_player(&mut app, 1);
            let second = (count == 2).then(|| standing_player(&mut app, 2).0);
            app.update();
            assert_switches(&app, true);
            leave(&mut app, 1, first);
            app.update();
            let reset = trigger == DeathTrigger::Any || (count == 1 && trigger != DeathTrigger::Never);
            assert_switches(&app, !reset);
            app.update();
            assert_switches(&app, !reset);
            if let Some(second) = second {
                leave(&mut app, 2, second);
                app.update();
                assert_switches(&app, trigger == DeathTrigger::Never);
            }
        }
    }
}

#[test]
fn logout_reset_wins_over_a_held_plate_until_a_fresh_press() {
    for activation in [PressureSwitchActivation::Toggle, PressureSwitchActivation::Auto] {
        let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
        configure_switches(&mut app, activation, DeathTrigger::Any);
        let (first, _) = standing_player(&mut app, 1);
        let (survivor, _) = standing_player(&mut app, 2);
        app.update();
        assert_switches(&app, true);
        app.add_systems(
            Update,
            (move |mut players: ResMut<PlayerMap>, mut commands: Commands| {
                if players.disconnect(&PlayerId(1), 2.0).is_some() {
                    commands.entity(first).despawn();
                }
            })
            .in_set(ServerSet::Ingress),
        );
        app.update();
        assert_switches(&app, false);
        app.update();
        assert_switches(&app, false);
        step_off(&mut app, survivor);
        app.update();
        step_on(&mut app, survivor);
        app.update();
        assert_switches(&app, true);
    }
}

#[test]
fn bridge_collision_loses_power_on_the_death_or_logout_tick() {
    for logout in [false, true] {
        let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
        configure_switches(&mut app, PressureSwitchActivation::Toggle, DeathTrigger::All);
        app.insert_resource(CollisionWorld::from_map_layout(
            &MapLayout {
                light_bridges: vec![LightBridge {
                    id: Default::default(),
                    switch: None,
                    switch_inverted: false,

                    x1: -1.0,
                    x2: 1.0,
                    z1: -1.0,
                    z2: 1.0,
                    y: 0.0,
                    thickness: 0.1,
                    level: 0,
                    kind: BridgeKindId(0),
                    carrier: CarrierId::WORLD,
                }],
                ..default()
            },
            &Default::default(),
        ));
        standing_player(&mut app, 1);
        let clear = |app: &App| {
            app.world()
                .resource::<CollisionWorld>()
                .attack_path_clear(Vec3::Y, Vec3::NEG_Y, &[])
        };
        app.update();
        assert!(!clear(&app));
        app.add_systems(
            Update,
            (move |mut players: ResMut<PlayerMap>, mut commands: Commands| {
                if logout {
                    let info = players.disconnect(&PlayerId(1), 2.0).expect("departing player missing");
                    commands
                        .entity(info.entity().expect("departing player entity missing"))
                        .despawn();
                } else {
                    players.begin_respawn(PlayerId(1), 2.0);
                }
            })
            .in_set(if logout {
                ServerSet::Ingress
            } else {
                ServerSet::CombatDamage
            }),
        );
        app.update();
        assert_switches(&app, false);
        assert!(clear(&app));
    }
}

#[test]
fn switches_choose_independent_activation_and_death_policies() {
    let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
    install_switches(
        &mut app,
        PressureSwitchConfig {
            activation: PressureSwitchActivation::Toggle,
            reset_on_player_death: DeathTrigger::Any,
            held: SwitchHold::Any,
        },
        PressureSwitchConfig {
            activation: PressureSwitchActivation::Momentary,
            ..Default::default()
        },
    );
    let (first, _) = standing_player(&mut app, 1);
    standing_player(&mut app, 2);
    app.update();
    assert_switches(&app, true);
    step_off(&mut app, first);
    die(&mut app, 1);
    app.update();
    assert!(open_kinds(&app).is_empty());
    assert_eq!(powered_kinds(&app), [SKYWAY]);
}

#[test]
fn a_different_plate_of_the_switch_can_toggle_it_while_the_first_stays_held() {
    let mut app = app(
        catalog(vec![]),
        vec![
            lobby_plate(),
            skyway_plate(),
            PressurePlateRuntime {
                col: 1,
                ..lobby_plate()
            },
            PressurePlateRuntime {
                col: 1,
                ..skyway_plate()
            },
        ],
    );
    configure_switches(&mut app, PressureSwitchActivation::Toggle, DeathTrigger::Never);
    standing_player(&mut app, 1);
    app.update();
    assert_switches(&app, true);
    let (second, _) = standing_player(&mut app, 2);
    step_onto_second_plate(&mut app, second);
    app.update();
    assert_switches(&app, false);
}

#[test]
fn toggle_switches_reset_before_a_dead_player_respawns() {
    let config = catalog(vec![]);
    let mut app = app(config.clone(), vec![lobby_plate(), skyway_plate()]);
    configure_switches(&mut app, PressureSwitchActivation::Toggle, DeathTrigger::All);
    app.world_mut()
        .resource_mut::<MapConfig>()
        .player_spawn_zones
        .push(PlayerSpawnZone {
            carrier: CarrierId::WORLD,
            level: 0,
            cols: [1, 2],
            rows: [1, 2],
        });
    app.insert_resource(config.gameplay_config())
        .insert_resource(PortalAssignments::new(PortalMode::Both))
        .init_resource::<ActorMap>()
        .init_resource::<ActorRespawnTimers>()
        .init_resource::<PendingActorSpawns>()
        .add_systems(Update, players_respawn_system.in_set(ServerSet::Lifecycle));
    let (entity, _) = standing_player(&mut app, 1);
    let switch_pos = *app.world().get::<Position>(entity).expect("player position missing");
    app.update();
    assert_switches(&app, true);
    app.world_mut()
        .run_system_once(move |mut commands: Commands, mut players: ResMut<PlayerMap>| {
            kill_player(
                &mut commands,
                &mut players,
                PlayerId(1),
                entity,
                switch_pos,
                0.0,
                DeathSource::Beam { kind: "turret".into() },
                &config,
                &mut PendingExplosions::default(),
            );
        })
        .expect("death system failed");
    app.update();
    assert_switches(&app, false);
    assert!(
        !app.world()
            .resource::<PlayerMap>()
            .get(&PlayerId(1))
            .expect("respawned player missing")
            .is_dead()
    );
}

#[test]
fn same_kind_fields_and_an_inverted_carrier_respond_per_target() {
    let mut layout = switched_carrier_layout();
    layout.carriers[0].switch_inverted = true;
    let mut app = app_with_layout(catalog(vec![]), vec![lobby_plate()], layout);
    {
        let mut layout = app.world_mut().resource_mut::<MapLayout>();
        let mut opposite = layout.barriers[0];
        opposite.id = BarrierId(1);
        opposite.switch_inverted = true;
        layout.barriers.push(opposite);
        opposite.id = BarrierId(2);
        opposite.switch = None;
        layout.barriers.push(opposite);
        layout.light_bridges[0].switch = Some(LOBBY_SWITCH);
        let mut opposite = layout.light_bridges[0];
        opposite.id = BridgeId(1);
        opposite.switch_inverted = true;
        layout.light_bridges.push(opposite);
    }
    configure_switches(&mut app, PressureSwitchActivation::Momentary, DeathTrigger::Never);
    app.update();
    assert_eq!(open_kinds(&app), [BarrierId(1)]);
    assert_eq!(powered_kinds(&app), [BridgeId(1)]);
    assert!(carrier_run(&app).running);
    let (player, _) = standing_player(&mut app, 1);
    app.update();
    assert_eq!(open_kinds(&app), [LOBBY]);
    assert_eq!(powered_kinds(&app), [SKYWAY]);
    assert!(!carrier_run(&app).running);
    step_off(&mut app, player);
    app.update();
    assert_eq!(open_kinds(&app), [BarrierId(1)]);
    assert_eq!(powered_kinds(&app), [BridgeId(1)]);
    assert!(carrier_run(&app).running);
}

#[test]
fn inverted_fireworks_wait_for_unlock_and_stop_while_the_kind_is_on() {
    let mut app = app(catalog(vec![]), vec![]);
    app.world_mut()
        .resource_mut::<MapFireworks>()
        .0
        .as_mut()
        .expect("fireworks missing")
        .switch_inverted = true;
    let mut switches = PressureSwitches::from_world(app.world_mut());
    assert!(!switches.fireworks_due(0, &[FIREWORKS_SWITCH]));
    assert!(switches.fireworks_due(0, &[]));
    let plates = [PressurePlateRuntime {
        switch: FIREWORKS_SWITCH,
        ..lobby_plate()
    }];
    switches.update(1, 1, HashSet::from([0]), &plates, 1);
    assert!(!switches.fireworks_due(10000, &[]));
    switches.update(1, 1, HashSet::new(), &plates, 10001);
    assert!(switches.fireworks_due(10001, &[]));
}
