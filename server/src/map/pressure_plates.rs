use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

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

// Per-tick plate occupancy, then the two plate rules.
//
// Holding plates — barrier plates open every barrier of their kind, bridge
// plates make every light bridge of their kind solid and lit. Both follow one
// rule; for each purpose that has at least one plate on the map:
//   required = min(plates_for_purpose, max(0, active_alive_count - 1))
// and the purpose is held while the number of distinct held plates of that
// purpose is `>= required`.
//
// Solo play (exactly one logged-in player, dead or alive) replaces that rule
// with switches: a fresh press flips its purpose on or off, and stepping off
// changes nothing. The switches start from the plates held when solo play
// begins — a purpose the remaining player was holding stays held — and are
// dropped once a second player logs in.
//
// Firework plates — required = min(firework_plates, active_alive_count);
// the show launches on the tick the held count reaches it (edge-triggered,
// so standing there doesn't restart it every tick). Fireworks are momentary
// (`PlatePurpose::held`), so they never enter `PlateState`.
//
// A plate is "held" when ≥ 1 alive player is inside the inner 25%-by-area
// square of its cell (see `player_on_plate`).
pub fn pressure_plates_system(
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    mut players: ResMut<PlayerMap>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    mut quest_board: ResMut<QuestBoard>,
    quest_catalog: Res<QuestCatalog>,
    barrier_kinds: Res<BarrierKindTable>,
    bridge_kinds: Res<BridgeKindTable>,
    positions: Query<&Position, With<PlayerMarker>>,
    mut plates_state: ResMut<PlateState>,
    // Plate indices held last tick. Fires `SPressurePlate` only on the
    // unpressed→pressed edge (step-on cue), not every tick a player keeps
    // standing, and tells a fresh press from a standing one for feed lines.
    mut prev_held: Local<HashSet<usize>>,
    // Holding-plate count per purpose. Derived from the immutable map, so it
    // is built once on first run rather than rebuilt every tick.
    mut plates_per_purpose: Local<HashMap<HeldPurpose, usize>>,
    // Whether the firework threshold held last tick; the show launches on
    // the false→true edge.
    mut fireworks_ready: Local<bool>,
    // Solo switch positions: the purposes a lone player has flipped on.
    // `None` outside solo play.
    mut switches: Local<Option<HashSet<HeldPurpose>>>,
) {
    if map_config.pressure_plates.is_empty() {
        plates_state.set_if_neq(PlateState::default());
        prev_held.clear();
        *fireworks_ready = false;
        *switches = None;
        return;
    }

    if plates_per_purpose.is_empty() {
        for purpose in map_config
            .pressure_plates
            .iter()
            .filter_map(|plate| plate.purpose.held())
        {
            *plates_per_purpose.entry(purpose).or_insert(0) += 1;
        }
    }

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

    // Per-tick: who holds each plate (the first alive player found on it).
    // Inactive plates are skipped outright, so they neither click nor count.
    let locked = quest_board.locked_plate_purposes().to_vec();
    let plates = &map_config.pressure_plates;
    let mut holders: HashMap<usize, PlayerId> = HashMap::new();
    for (idx, plate) in plates.iter().enumerate() {
        if !plate_active(plate, &locked) {
            continue;
        }
        let geometry = &map_config.grid(plate.carrier).geometry;
        let pose = carriers.pose(plate.carrier);
        let holder = players.iter().find(|(_, info)| {
            info.connection.logged_in
                && info
                    .entity()
                    .and_then(|entity| positions.get(entity).ok())
                    .is_some_and(|pos| {
                        let local = pose.inverse_transform_position(pos);
                        player_on_plate(plate, &local, geometry)
                    })
        });
        if let Some((id, _)) = holder {
            holders.insert(idx, *id);
        }
    }
    let held_indices: HashSet<usize> = holders.keys().copied().collect();
    let held_per_purpose = held_count_per_purpose(&held_indices, plates);
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

    // Purposes a solo press flipped this tick.
    let mut flipped = Vec::new();
    let mut next: Vec<HeldPurpose> = if logged_in == 1 {
        let seeded = switches.is_none();
        let switches = switches.get_or_insert_with(|| held_per_purpose.keys().copied().collect());
        if !seeded {
            for purpose in held_indices
                .difference(&prev_held)
                .filter_map(|idx| plates[*idx].purpose.held())
            {
                if !switches.remove(&purpose) {
                    switches.insert(purpose);
                }
                flipped.push(purpose);
            }
        }
        switches.iter().copied().collect()
    } else {
        *switches = None;
        let mut next = Vec::new();
        for (purpose, plates_for_purpose) in plates_per_purpose.iter() {
            let required = (*plates_for_purpose).min(alive.saturating_sub(1));
            let held = held_per_purpose.get(purpose).copied().unwrap_or(0);
            if held >= required {
                next.push(*purpose);
            }
        }
        next
    };
    // `PlateState` keeps sorted lists, so the equality check at the end only
    // holds if this is sorted too; without it the HashMap order would rewrite
    // the resource every tick.
    next.sort();

    // Feed lines follow plate presses only. The alive-count term and the
    // switch-over into or out of solo play also flip purposes (joins,
    // leaves, deaths); those stay silent — the barriers and bridges
    // themselves already show it.
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
    if ready && !*fireworks_ready {
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
    *fireworks_ready = ready;

    *prev_held = held_indices;
    // The bridge collider sync (`powered_bridges_sync_system`) and the
    // client's barrier visibility react to a change, so an equal state must
    // not count as one.
    plates_state.set_if_neq(PlateState::from_held(next));
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
// nobody is on a plate of that purpose — the alive-count term flipped it.
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
    use bevy::prelude::*;
    use common::{map::Carriers, protocol::CarrierId};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    use super::*;
    use crate::{
        config::{QuestKind, ServerGameplayConfig},
        map::{CellGrid, EdgeGrid, LevelGrid},
        network::ServerToClient,
        players::PlayerInfo,
        quests::{
            QuestCatalog,
            test_support::{catalog, completed, drain, feed_lines, quest},
        },
        test_geometry::geometry,
    };
    use common::protocol::{BarrierKindId, BridgeKindId, QuestId, QuestScope};

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
        let mut app = App::new();
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
            .add_systems(Update, pressure_plates_system);
        app
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
            quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None),
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
        assert_eq!(open_kinds(&app), [LOBBY], "an empty server holds every plate kind open");

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
}
