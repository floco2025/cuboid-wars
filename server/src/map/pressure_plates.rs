use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::pressure_switches::PressureSwitches;

use crate::{
    config::ServerGameplayConfig,
    map::{MapConfig, PressurePlateRuntime},
    network::{FeedAudience, FeedEvent, broadcast_firework_show, broadcast_to_all, emit_feed},
    players::PlayerMap,
    quests::{QuestBoard, QuestCatalog, QuestEvent, record_event},
};
use common::{
    map::{Carriers, MapGeometry},
    protocol::{
        BarrierKindTable, BridgeKindTable, HeldPurpose, PlatePurpose, PlateState, PlayerId, PlayerMarker, Position,
        SPressurePlate, ServerMessage,
    },
};

// Is `pos`, in the plate's carrier frame, inside this plate's inner
// 25%-by-area square AND on the plate's level? Y matches within half a
// storey of the plate's floor, which keeps a player on the floor above from
// triggering a plate one level down.
#[must_use]
pub fn player_on_plate(plate: &PressurePlateRuntime, pos: &Position, geometry: &MapGeometry) -> bool {
    if (pos.y - geometry.level_y(plate.level)).abs() >= geometry.level_height() / 2.0 {
        return false;
    }
    let cell = geometry.cell_size();
    let cell_x = geometry.cell_to_world_x(plate.col);
    let cell_z = geometry.cell_to_world_z(plate.row);
    let min_x = cell_x + cell * 0.25;
    let max_x = cell_x + cell * 0.75;
    let min_z = cell_z + cell * 0.25;
    let max_z = cell_z + cell * 0.75;
    pos.x >= min_x && pos.x <= max_x && pos.z >= min_z && pos.z <= max_z
}

// Barrier and bridge kinds use their configured activation: any occupied plate
// for momentary, each fresh plate press for toggle, and toggle with exactly one
// logged-in player for auto. Entering auto toggle seeds from current occupancy.
// Fireworks require min(plate count, alive player count), with at least one alive
// player, and fire only on the threshold's rising edge.
pub(super) fn pressure_plates_system(
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    mut players: ResMut<PlayerMap>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    mut quest_board: ResMut<QuestBoard>,
    quest_catalog: Res<QuestCatalog>,
    barrier_kinds: Res<BarrierKindTable>,
    bridge_kinds: Res<BridgeKindTable>,
    positions: Query<&Position, With<PlayerMarker>>,
    plates_state: Res<PlateState>,
    mut switches: ResMut<PressureSwitches>,
) {
    let mut logged_in: usize = 0;
    let mut alive: usize = 0;
    for (_, info) in players.iter() {
        if info.connection.logged_in {
            logged_in += 1;
            if !info.is_dead() {
                alive += 1;
            }
        }
    }

    let plates = &map_config.pressure_plates;
    let holders = plate_holders(
        &map_config,
        &carriers,
        &players,
        &positions,
        quest_board.locked_plate_purposes(),
    );
    let held_indices: HashSet<usize> = holders.keys().copied().collect();
    let held_per_purpose = held_count_per_purpose(&held_indices, plates);
    let prev_held = switches.prev_held.clone();
    let prev_held_per_purpose = held_count_per_purpose(&prev_held, plates);

    // Edge-triggered cues: at most one press and one release cue per tick,
    // regardless of how many plates flipped — the messages carry no plate
    // identity, so collapsing simultaneous flips is lossless. Persistent state
    // lives in `PlateState` + snapshot; these are pure click/clunk SFX.
    if held_indices.difference(&prev_held).next().is_some() {
        broadcast_to_all(&players, ServerMessage::PressurePlate(SPressurePlate { pressed: true }));
    }
    if prev_held.difference(&held_indices).next().is_some() {
        broadcast_to_all(
            &players,
            ServerMessage::PressurePlate(SPressurePlate { pressed: false }),
        );
    }

    let flipped = switches.update(logged_in, &held_indices, plates);
    let next_state = switches.state();
    let next: Vec<_> = next_state.held().collect();

    let kind_name = |purpose: HeldPurpose| match purpose {
        HeldPurpose::Barrier(kind) => barrier_kinds
            .id(kind)
            .expect("barrier kind missing from BarrierKindTable")
            .to_owned(),
        HeldPurpose::Bridge(kind) => bridge_kinds
            .id(kind)
            .expect("bridge kind missing from BridgeKindTable")
            .to_owned(),
    };
    for purpose in next.iter().copied().filter(|purpose| !plates_state.contains(*purpose)) {
        let Some(presser) = presser_of_purpose(purpose, &holders, &prev_held, plates) else {
            continue;
        };
        let name = players.display_name(&presser);
        emit_feed(
            &players,
            &server_gameplay_config.feed,
            FeedAudience::Everyone,
            FeedEvent::plate_held(purpose, name, kind_name(purpose)),
        );
    }
    for purpose in plates_state.held().filter(|purpose| !next.contains(purpose)) {
        let held_now = held_per_purpose.get(&purpose).copied().unwrap_or(0);
        let held_before = prev_held_per_purpose.get(&purpose).copied().unwrap_or(0);
        if !flipped.contains(&purpose) && held_now >= held_before {
            continue;
        }
        emit_feed(
            &players,
            &server_gameplay_config.feed,
            FeedAudience::Everyone,
            FeedEvent::plate_released(purpose, kind_name(purpose)),
        );
    }

    let firework_plates = plates
        .iter()
        .filter(|plate| plate.purpose == PlatePurpose::Firework)
        .count();
    let held_fireworks = held_indices
        .iter()
        .filter(|idx| plates[**idx].purpose == PlatePurpose::Firework)
        .count();
    let ready = firework_plates_ready(firework_plates, held_fireworks, alive);
    if ready && !switches.fireworks_ready {
        broadcast_firework_show(&players);
        // `/firework` bypasses this on purpose: only the plates count.
        record_event(
            &mut players,
            &mut quest_board,
            &quest_catalog,
            &server_gameplay_config.feed,
            QuestEvent::FireworksStarted,
        );
    }
    switches.fireworks_ready = ready;

    switches.prev_held = held_indices;
}

pub(super) fn pressure_switch_death_reset_system(
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    mut players: ResMut<PlayerMap>,
    positions: Query<&Position, With<PlayerMarker>>,
    quest_board: Res<QuestBoard>,
    mut switches: ResMut<PressureSwitches>,
) {
    let deaths = players.take_deaths();
    if deaths.is_empty() {
        return;
    }
    let logged_in = players.values().filter(|info| info.connection.logged_in).count();
    let plates = &map_config.pressure_plates;
    let holders = plate_holders(
        &map_config,
        &carriers,
        &players,
        &positions,
        quest_board.locked_plate_purposes(),
    );
    let held: HashSet<_> = holders.keys().copied().collect();
    let mut reset = HashSet::new();
    for (purpose, switch) in &mut switches.kinds {
        let occupied = held.iter().any(|idx| plates[*idx].purpose.held() == Some(*purpose));
        switch.update_mode(logged_in, occupied);
        if switch.toggle
            && deaths.iter().any(|death| {
                switch
                    .config
                    .reset_on_player_death
                    .applies(death.logged_in, death.alive)
            })
        {
            switch.active = false;
            reset.insert(*purpose);
        }
    }
    // Consume presses on the death tick so reset wins even for a surviving holder.
    switches.prev_held.retain(|idx| {
        !plates[*idx]
            .purpose
            .held()
            .is_some_and(|purpose| reset.contains(&purpose))
    });
    switches.prev_held.extend(held.into_iter().filter(|idx| {
        plates[*idx]
            .purpose
            .held()
            .is_some_and(|purpose| reset.contains(&purpose))
    }));
}

fn plate_holders(
    map_config: &MapConfig,
    carriers: &Carriers,
    players: &PlayerMap,
    positions: &Query<&Position, With<PlayerMarker>>,
    locked: &[PlatePurpose],
) -> HashMap<usize, PlayerId> {
    let mut holders = HashMap::new();
    for (idx, plate) in map_config.pressure_plates.iter().enumerate() {
        if !plate_active(plate, locked) {
            continue;
        }
        let geometry = &map_config.grid(plate.carrier).geometry;
        let pose = carriers.pose(plate.carrier);
        let holder = players.iter().find(|(_, info)| {
            info.connection.logged_in
                && info
                    .entity()
                    .and_then(|entity| positions.get(entity).ok())
                    .is_some_and(|pos| player_on_plate(plate, &pose.inverse_transform_position(pos), geometry))
        });
        if let Some((id, _)) = holder {
            holders.insert(idx, *id);
        }
    }
    holders
}

// Everyone alive is on a firework plate — or every plate is held when the
// players outnumber them.
fn firework_plates_ready(plates: usize, held: usize, alive: usize) -> bool {
    plates > 0 && alive > 0 && held >= plates.min(alive)
}

// Plates that solve a still-locked quest don't exist for the players yet.
fn plate_active(plate: &PressurePlateRuntime, locked: &[PlatePurpose]) -> bool {
    !locked.contains(&plate.purpose)
}

fn held_count_per_purpose(held: &HashSet<usize>, plates: &[PressurePlateRuntime]) -> HashMap<HeldPurpose, usize> {
    let mut counts = HashMap::new();
    for purpose in held.iter().filter_map(|idx| plates[*idx].purpose.held()) {
        *counts.entry(purpose).or_insert(0) += 1;
    }
    counts
}

// Who gets credit for flipping a purpose on: the holder of one of its
// plates that was not held last tick, else any current holder. `None` when
// nobody is on a plate of that purpose.
fn presser_of_purpose(
    purpose: HeldPurpose,
    holders: &HashMap<usize, PlayerId>,
    prev_held: &HashSet<usize>,
    plates: &[PressurePlateRuntime],
) -> Option<PlayerId> {
    let mut standing = None;
    for (idx, id) in holders {
        if plates[*idx].purpose.held() != Some(purpose) {
            continue;
        }
        if !prev_held.contains(idx) {
            return Some(*id);
        }
        standing = Some(*id);
    }
    standing
}

#[cfg(test)]
mod player_on_plate_tests {
    use super::*;
    use crate::test_geometry::{CELL, LEVEL_HEIGHT, geometry};
    use common::protocol::{BarrierKindId, CarrierId};

    fn make_plate(level: u8, col: i32, row: i32) -> PressurePlateRuntime {
        PressurePlateRuntime {
            carrier: CarrierId::WORLD,
            level,
            col,
            row,
            purpose: PlatePurpose::Barrier(BarrierKindId(0)),
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
}

#[cfg(test)]
mod presser_tests {
    use super::*;
    use common::protocol::{BarrierKindId, CarrierId};

    #[test]
    fn presser_prefers_a_fresh_press_over_a_standing_holder() {
        let plates = vec![
            PressurePlateRuntime {
                carrier: CarrierId::WORLD,
                level: 0,
                col: 0,
                row: 0,
                purpose: PlatePurpose::Barrier(BarrierKindId(0)),
            },
            PressurePlateRuntime {
                carrier: CarrierId::WORLD,
                level: 0,
                col: 1,
                row: 0,
                purpose: PlatePurpose::Barrier(BarrierKindId(0)),
            },
            PressurePlateRuntime {
                carrier: CarrierId::WORLD,
                level: 0,
                col: 2,
                row: 0,
                purpose: PlatePurpose::Barrier(BarrierKindId(1)),
            },
        ];
        let holders = HashMap::from([(0, PlayerId(1)), (1, PlayerId(2)), (2, PlayerId(3))]);
        let prev_held = HashSet::from([0]);

        assert_eq!(
            presser_of_purpose(HeldPurpose::Barrier(BarrierKindId(0)), &holders, &prev_held, &plates),
            Some(PlayerId(2))
        );
        assert_eq!(
            presser_of_purpose(HeldPurpose::Barrier(BarrierKindId(1)), &holders, &prev_held, &plates),
            Some(PlayerId(3))
        );
        assert_eq!(
            presser_of_purpose(HeldPurpose::Barrier(BarrierKindId(2)), &holders, &prev_held, &plates),
            None
        );
    }
}

#[cfg(test)]
mod firework_tests {
    use super::*;

    #[test]
    fn firework_needs_every_player_when_plates_suffice() {
        assert!(!firework_plates_ready(3, 1, 2));
        assert!(firework_plates_ready(3, 2, 2));
        assert!(firework_plates_ready(1, 1, 1));
    }

    #[test]
    fn firework_needs_every_plate_when_players_outnumber_them() {
        assert!(!firework_plates_ready(2, 1, 5));
        assert!(firework_plates_ready(2, 2, 5));
    }

    #[test]
    fn no_firework_plates_never_fire() {
        assert!(!firework_plates_ready(0, 0, 3));
    }

    #[test]
    fn no_players_never_fire() {
        assert!(!firework_plates_ready(2, 0, 0));
    }
}

#[cfg(test)]
mod system_tests {
    use bevy::{ecs::system::RunSystemOnce, prelude::*};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    use super::*;
    use crate::{
        actors::{ActorMap, ActorRespawnTimers, PendingActorSpawns},
        combat::{DeathSource, PendingExplosions, kill_player},
        config::{LightingMode, PlayerRespawnMode, QuestKind, RespawnConfig, ServerGameplayConfig, WeatherMode},
        map::{CellGrid, EdgeGrid, LevelGrid, LightState, WeatherState, generate_map, map_plugin},
        network::ServerToClient,
        players::{PlayerInfo, players_group_respawn_system, players_respawn_system},
        quests::{
            QuestCatalog,
            test_support::{catalog, completed, drain, feed_lines, quest},
        },
        schedule::{ServerSet, configure_server_schedule},
        test_geometry::geometry,
    };
    use common::{
        config::{DeathTrigger, PressureSwitchActivation, PressureSwitchConfig},
        map::Carriers,
        physics::CollisionWorld,
        protocol::{
            BarrierKindId, BridgeKindId, CarrierId, HexColor, KindDef, LightBridge, MapLayout, QuestId, QuestScope,
        },
    };

    const LOBBY: BarrierKindId = BarrierKindId(0);
    const SKYWAY: BridgeKindId = BridgeKindId(0);

    fn firework_plate() -> PressurePlateRuntime {
        PressurePlateRuntime {
            carrier: CarrierId::WORLD,
            level: 0,
            col: 0,
            row: 0,
            purpose: PlatePurpose::Firework,
        }
    }

    fn lobby_plate() -> PressurePlateRuntime {
        PressurePlateRuntime {
            purpose: PlatePurpose::Barrier(LOBBY),
            ..firework_plate()
        }
    }

    fn skyway_plate() -> PressurePlateRuntime {
        PressurePlateRuntime {
            purpose: PlatePurpose::Bridge(SKYWAY),
            ..firework_plate()
        }
    }

    fn app(config: ServerGameplayConfig, plates: Vec<PressurePlateRuntime>) -> App {
        let quest_catalog = QuestCatalog::from_config(&config);
        let board = QuestBoard::from_catalog(&quest_catalog);
        let mut settings = config.maps[&config.default_map].settings.clone();
        settings.barrier_kinds = vec![kind("lobby")];
        settings.bridge_kinds = vec![kind("skyway")];
        let mut app = App::new();
        app.insert_resource(WeatherState::new(config.cycles.weather.clone(), WeatherMode::Clear))
            .insert_resource(LightState::new(config.cycles.lighting.clone(), LightingMode::Bright))
            .insert_resource(settings)
            .insert_resource(CollisionWorld::from_map_layout(
                &MapLayout::default(),
                &Default::default(),
            ));
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
            .insert_resource(Carriers::default())
            .insert_resource(PlayerMap::default())
            .insert_resource(config)
            .insert_resource(quest_catalog)
            .insert_resource(board)
            .insert_resource(BarrierKindTable::from_ids(vec!["lobby".to_owned()]).expect("one barrier kind"))
            .insert_resource(BridgeKindTable::from_ids(vec!["skyway".to_owned()]).expect("one bridge kind"))
            .insert_resource(PlateState::default())
            .add_plugins(map_plugin);
        configure_server_schedule(&mut app);
        app.add_systems(Update, players_group_respawn_system.in_set(ServerSet::Lifecycle));
        app
    }

    fn kind(id: &str) -> KindDef {
        KindDef {
            id: id.to_owned(),
            color: HexColor([0; 3]),
            pressure_switch: Default::default(),
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
        app.world_mut().resource_mut::<PlayerMap>().remove(&PlayerId(id));
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

    fn open_kinds(app: &App) -> Vec<BarrierKindId> {
        app.world().resource::<PlateState>().open_barrier_kinds.clone()
    }

    fn powered_kinds(app: &App) -> Vec<BridgeKindId> {
        app.world().resource::<PlateState>().powered_bridge_kinds.clone()
    }

    fn barrier_lines(messages: &[ServerMessage]) -> Vec<String> {
        feed_lines(messages)
            .into_iter()
            .filter(|line| line.contains("barriers"))
            .collect()
    }

    fn bridge_lines(messages: &[ServerMessage]) -> Vec<String> {
        feed_lines(messages)
            .into_iter()
            .filter(|line| line.contains("bridges"))
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
    fn an_unlocked_plate_fires_once_per_press() {
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

        // Step off, then back on: the show fires again, the latched quest doesn't.
        step_off(&mut app, entity);
        app.update();
        assert_eq!(clicks(&drain(&mut rx)), 1, "release click");
        step_on(&mut app, entity);
        app.update();
        let messages = drain(&mut rx);
        assert_eq!(shows(&messages), 1);
        assert!(!completed(&messages, "show"));
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
        let lines = barrier_lines(&drain(&mut rx));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("opened the lobby"), "{lines:?}");

        step_off(&mut app, entity);
        app.update();
        assert_eq!(open_kinds(&app), [LOBBY], "stepping off leaves the switch alone");
        assert!(barrier_lines(&drain(&mut rx)).is_empty());

        step_on(&mut app, entity);
        app.update();
        assert!(open_kinds(&app).is_empty());
        assert_eq!(barrier_lines(&drain(&mut rx)), ["The lobby barriers closed"]);
    }

    #[test]
    fn a_first_login_prints_no_closed_lines() {
        let mut app = app(catalog(Vec::new()), vec![lobby_plate()]);
        app.update();
        assert!(open_kinds(&app).is_empty(), "empty plates stay off on an empty server");

        let (entity, mut rx) = standing_player(&mut app, 1);
        step_off(&mut app, entity);
        app.update();
        assert!(open_kinds(&app).is_empty());
        assert!(barrier_lines(&drain(&mut rx)).is_empty());
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
        let lines = bridge_lines(&drain(&mut rx));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("powered the skyway"), "{lines:?}");

        step_off(&mut app, entity);
        app.update();
        assert!(powered_kinds(&app).is_empty());
        assert_eq!(bridge_lines(&drain(&mut rx)), ["The skyway bridges went dark"]);
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
        assert!(barrier_lines(&drain(&mut rx)).is_empty());
    }

    fn configure_switches(app: &mut App, activation: PressureSwitchActivation, trigger: DeathTrigger) {
        let mut barrier = kind("lobby");
        let mut bridge = kind("skyway");
        barrier.pressure_switch = PressureSwitchConfig {
            activation,
            reset_on_player_death: trigger,
        };
        bridge.pressure_switch = barrier.pressure_switch;
        app.insert_resource(PressureSwitches::new(&[barrier], &[bridge]));
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
        configure_switches(&mut app, PressureSwitchActivation::Toggle, DeathTrigger::All);
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
    fn all_reset_does_not_follow_a_disconnect_and_solo_uses_counts_at_death() {
        for trigger in [DeathTrigger::All, DeathTrigger::Solo] {
            let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
            configure_switches(&mut app, PressureSwitchActivation::Toggle, trigger);
            standing_player(&mut app, 1);
            let (partner, _) = standing_player(&mut app, 2);
            app.update();
            die(&mut app, 1);
            leave(&mut app, 2, partner);
            app.update();
            assert_switches(&app, true);
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
    fn bridge_collision_loses_power_on_the_death_tick() {
        let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
        configure_switches(&mut app, PressureSwitchActivation::Toggle, DeathTrigger::All);
        app.insert_resource(CollisionWorld::from_map_layout(
            &MapLayout {
                light_bridges: vec![LightBridge {
                    x1: -1.0,
                    x2: 1.0,
                    z1: -1.0,
                    z2: 1.0,
                    y: 0.0,
                    thickness: 0.1,
                    level: 0,
                    kind: SKYWAY,
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
            (|mut players: ResMut<PlayerMap>| {
                players.begin_respawn(PlayerId(1), 2.0);
            })
            .in_set(ServerSet::CombatDamage),
        );
        app.update();
        assert_switches(&app, false);
        assert!(clear(&app));
    }

    #[test]
    fn kinds_choose_independent_activation_and_death_policies() {
        let mut app = app(catalog(vec![]), vec![lobby_plate(), skyway_plate()]);
        let mut barrier = kind("lobby");
        barrier.pressure_switch = PressureSwitchConfig {
            activation: PressureSwitchActivation::Toggle,
            reset_on_player_death: DeathTrigger::Any,
        };
        let mut bridge = kind("skyway");
        bridge.pressure_switch.activation = PressureSwitchActivation::Momentary;
        app.insert_resource(PressureSwitches::new(&[barrier], &[bridge]));
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
    fn a_different_matching_plate_can_toggle_a_kind_while_the_first_stays_held() {
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
        let cell = app.world().resource::<MapGeometry>().cell_size();
        app.world_mut()
            .get_mut::<Position>(second)
            .expect("second player position missing")
            .x += cell;
        app.update();
        assert_switches(&app, false);
    }

    #[test]
    fn puzzle_access_closes_its_barrier_before_the_dead_player_respawns() {
        let config = ServerGameplayConfig::load_default().expect("gameplay config rejected");
        let settings = config.maps["puzzle_access"].settings.clone();
        let (barriers, bridges) = settings.kind_tables().expect("access kind catalogs rejected");
        let generated = generate_map("puzzle_access", &settings, &barriers, &bridges).expect("access map rejected");
        let geometry = generated.config.root_grid().geometry;
        let mut app = app(config.clone(), vec![]);
        app.insert_resource(PressureSwitches::new(&settings.barrier_kinds, &settings.bridge_kinds))
            .insert_resource(CollisionWorld::from_map_layout(&generated.layout, &barriers))
            .insert_resource(generated.config)
            .insert_resource(geometry)
            .insert_resource(barriers)
            .insert_resource(bridges)
            .insert_resource(config.gameplay_config())
            .init_resource::<ActorMap>()
            .init_resource::<ActorRespawnTimers>()
            .init_resource::<PendingActorSpawns>()
            .add_systems(Update, players_respawn_system.in_set(ServerSet::Lifecycle));
        let (entity, _) = standing_player(&mut app, 1);
        let switch_pos = Position {
            x: geometry.cell_center_x(2),
            y: 0.0,
            z: geometry.cell_center_z(2),
        };
        *app.world_mut()
            .get_mut::<Position>(entity)
            .expect("player position missing") = switch_pos;
        app.update();
        assert_eq!(open_kinds(&app), [LOBBY]);
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
                    &config.feed,
                    &mut PendingExplosions::default(),
                );
            })
            .expect("death system failed");
        app.update();
        assert!(open_kinds(&app).is_empty());
        assert!(
            !app.world()
                .resource::<PlayerMap>()
                .get(&PlayerId(1))
                .expect("respawned player missing")
                .is_dead()
        );
        let from = Vec3::new(geometry.cell_center_x(8), 1.15, geometry.cell_center_z(2));
        let to = Vec3::new(switch_pos.x, 1.15, switch_pos.z);
        assert!(
            !app.world()
                .resource::<CollisionWorld>()
                .attack_path_clear(from, to, &open_kinds(&app))
        );
    }
}
