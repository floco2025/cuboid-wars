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
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    map::MapGeometry,
    physics::OpenBarrierKinds,
    protocol::{
        BarrierKindId, BarrierKindTable, PlatePurpose, PlayerId, PlayerMarker, Position, SPressurePlate, ServerMessage,
    },
};

// World-space test: is `pos` inside this plate's inner 25%-by-area square AND
// on the plate's level? Y matches when `|pos.y - level * LEVEL_HEIGHT| <
// LEVEL_HEIGHT / 2`, which keeps a player on the floor above from triggering
// a plate one level down.
#[must_use]
pub fn player_on_plate(plate: &PressurePlateRuntime, pos: &Position, geometry: &MapGeometry) -> bool {
    let plate_y = f32::from(plate.level) * LEVEL_HEIGHT;
    if (pos.y - plate_y).abs() >= LEVEL_HEIGHT / 2.0 {
        return false;
    }
    let cell_x = geometry.cell_to_world_x(plate.col);
    let cell_z = geometry.cell_to_world_z(plate.row);
    let min_x = cell_x + GRID_CELL_SIZE * 0.25;
    let max_x = cell_x + GRID_CELL_SIZE * 0.75;
    let min_z = cell_z + GRID_CELL_SIZE * 0.25;
    let max_z = cell_z + GRID_CELL_SIZE * 0.75;
    pos.x >= min_x && pos.x <= max_x && pos.z >= min_z && pos.z <= max_z
}

// Per-tick plate occupancy, then the two plate rules.
//
// Barrier plates — for each kind that has at least one plate on the map:
//   required = min(plates_for_kind, max(0, active_alive_count - 1))
// and the kind is open while the number of distinct held plates of that
// kind is `>= required`.
//
// Firework plates — required = min(firework_plates, active_alive_count);
// the show launches on the tick the held count reaches it (edge-triggered,
// so standing there doesn't restart it every tick).
//
// A plate is "held" when ≥ 1 alive player is inside the inner 25%-by-area
// square of its cell (see `player_on_plate`).
pub fn pressure_plates_system(
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
    mut players: ResMut<PlayerMap>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    mut quest_board: ResMut<QuestBoard>,
    quest_catalog: Res<QuestCatalog>,
    barrier_kinds: Res<BarrierKindTable>,
    positions: Query<&Position, With<PlayerMarker>>,
    mut open: ResMut<OpenBarrierKinds>,
    // Plate indices held last tick. Fires `SPressurePlate` only on the
    // unpressed→pressed edge (step-on cue), not every tick a player keeps
    // standing, and tells a fresh press from a standing one for feed lines.
    mut prev_held: Local<HashSet<usize>>,
    // Barrier plate count per kind. Derived from the immutable map, so it is
    // built once on first run rather than rebuilt every tick.
    mut plates_per_kind: Local<HashMap<BarrierKindId, usize>>,
    // Whether the firework threshold held last tick; the show launches on
    // the false→true edge.
    mut fireworks_ready: Local<bool>,
) {
    if map_config.pressure_plates.is_empty() {
        if !open.0.is_empty() {
            open.0.clear();
        }
        prev_held.clear();
        *fireworks_ready = false;
        return;
    }

    if plates_per_kind.is_empty() {
        for plate in &map_config.pressure_plates {
            if let PlatePurpose::Barrier(kind) = plate.purpose {
                *plates_per_kind.entry(kind).or_insert(0) += 1;
            }
        }
    }

    let alive: usize = players
        .iter()
        .filter(|(_, info)| info.connection.logged_in && !info.is_dead())
        .count();

    // Per-tick: who holds each plate (the first alive player found on it).
    // Inactive plates are skipped outright, so they neither click nor count.
    let locked = quest_board.locked_plate_purposes().to_vec();
    let plates = &map_config.pressure_plates;
    let mut holders: HashMap<usize, PlayerId> = HashMap::new();
    for (idx, plate) in plates.iter().enumerate() {
        if !plate_active(plate, &locked) {
            continue;
        }
        let holder = players.iter().find(|(_, info)| {
            info.connection.logged_in
                && info
                    .entity()
                    .and_then(|entity| positions.get(entity).ok())
                    .is_some_and(|pos| player_on_plate(plate, pos, &map_geometry))
        });
        if let Some((id, _)) = holder {
            holders.insert(idx, *id);
        }
    }
    let held_indices: HashSet<usize> = holders.keys().copied().collect();
    let held_per_kind = held_count_per_kind(&held_indices, plates);
    let prev_held_per_kind = held_count_per_kind(&prev_held, plates);

    // Edge-triggered cues: at most one press and one release cue per tick,
    // regardless of how many plates flipped — the messages carry no plate
    // identity, so collapsing simultaneous flips is lossless. Persistent state
    // lives in `OpenBarrierKinds` + snapshot; these are pure click/clunk SFX.
    if held_indices.difference(&prev_held).next().is_some() {
        broadcast_to_all(&players, ServerMessage::PressurePlate(SPressurePlate { pressed: true }));
    }
    if prev_held.difference(&held_indices).next().is_some() {
        broadcast_to_all(
            &players,
            ServerMessage::PressurePlate(SPressurePlate { pressed: false }),
        );
    }

    let mut next = Vec::new();
    for (kind, plates_for_kind) in plates_per_kind.iter() {
        let required = (*plates_for_kind).min(alive.saturating_sub(1));
        let held = held_per_kind.get(kind).copied().unwrap_or(0);
        if held >= required {
            next.push(*kind);
        }
    }
    // Stable order for the equality diff below — without it, the HashMap
    // iteration order varies tick-to-tick and we'd rewrite the resource
    // every tick (defeating change detection on both server broadcast and
    // client visibility).
    next.sort_by_key(|k| k.0);

    // Feed lines follow plate presses only. The alive-count term also opens
    // and closes kinds (solo play, joins, deaths); those stay silent — the
    // barriers themselves already show it.
    let kind_name = |kind: BarrierKindId| {
        barrier_kinds
            .id(kind)
            .expect("barrier kind missing from BarrierKindTable")
            .to_owned()
    };
    let (opened, closed) = barrier_transitions(&open.0, &next);
    for kind in opened {
        if let Some(presser) = presser_of_kind(kind, &holders, &prev_held, plates) {
            emit_feed(
                &players,
                &server_gameplay_config.feed,
                FeedAudience::Everyone,
                FeedEvent::BarrierOpened {
                    name: players.display_name(&presser),
                    kind,
                    kind_name: kind_name(kind),
                },
            );
        }
    }
    for kind in closed {
        let held_now = held_per_kind.get(&kind).copied().unwrap_or(0);
        let held_before = prev_held_per_kind.get(&kind).copied().unwrap_or(0);
        if held_now < held_before {
            emit_feed(
                &players,
                &server_gameplay_config.feed,
                FeedAudience::Everyone,
                FeedEvent::BarrierClosed {
                    kind,
                    kind_name: kind_name(kind),
                },
            );
        }
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
    if next != open.0 {
        open.0 = next;
    }
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

fn held_count_per_kind(held: &HashSet<usize>, plates: &[PressurePlateRuntime]) -> HashMap<BarrierKindId, usize> {
    let mut counts = HashMap::new();
    for idx in held {
        if let PlatePurpose::Barrier(kind) = plates[*idx].purpose {
            *counts.entry(kind).or_insert(0) += 1;
        }
    }
    counts
}

// Who gets credit for opening a kind: the holder of one of its plates that
// was not held last tick, else any current holder. `None` when nobody is on
// a plate of that kind — the alive-count term opened it.
fn presser_of_kind(
    kind: BarrierKindId,
    holders: &HashMap<usize, PlayerId>,
    prev_held: &HashSet<usize>,
    plates: &[PressurePlateRuntime],
) -> Option<PlayerId> {
    let mut standing = None;
    for (idx, id) in holders {
        if plates[*idx].purpose != PlatePurpose::Barrier(kind) {
            continue;
        }
        if !prev_held.contains(idx) {
            return Some(*id);
        }
        standing = Some(*id);
    }
    standing
}

// (kinds in `next` but not `prev`, kinds in `prev` but not `next`).
fn barrier_transitions(prev: &[BarrierKindId], next: &[BarrierKindId]) -> (Vec<BarrierKindId>, Vec<BarrierKindId>) {
    let opened = next.iter().copied().filter(|kind| !prev.contains(kind)).collect();
    let closed = prev.iter().copied().filter(|kind| !next.contains(kind)).collect();
    (opened, closed)
}

#[cfg(test)]
mod player_on_plate_tests {
    use super::*;
    use common::constants::GRID_CELL_SIZE;

    fn make_plate(level: u8, col: i32, row: i32) -> PressurePlateRuntime {
        PressurePlateRuntime {
            level,
            col,
            row,
            purpose: PlatePurpose::Barrier(BarrierKindId(0)),
        }
    }

    // Grid 1x1 centers the world origin on the cell at (0, 0), so the plate
    // covers world-x in [-GRID_CELL_SIZE/2, GRID_CELL_SIZE/2] and the inner-
    // 50% rect is [-GRID_CELL_SIZE/4, GRID_CELL_SIZE/4] on each axis.
    fn geom() -> MapGeometry {
        MapGeometry::new(1, 1)
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
        let just_inside = -0.25 * GRID_CELL_SIZE + 0.01;
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
        let outside_x = -0.25 * GRID_CELL_SIZE - 0.01;
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
            x: GRID_CELL_SIZE / 2.0,
            y: 0.0,
            z: GRID_CELL_SIZE / 2.0,
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
mod transition_tests {
    use super::*;

    #[test]
    fn barrier_transitions_reports_opened_and_closed_kinds() {
        let (opened, closed) = barrier_transitions(
            &[BarrierKindId(0), BarrierKindId(1)],
            &[BarrierKindId(1), BarrierKindId(2)],
        );
        assert_eq!(opened, vec![BarrierKindId(2)]);
        assert_eq!(closed, vec![BarrierKindId(0)]);
    }

    #[test]
    fn presser_prefers_a_fresh_press_over_a_standing_holder() {
        let plates = vec![
            PressurePlateRuntime {
                level: 0,
                col: 0,
                row: 0,
                purpose: PlatePurpose::Barrier(BarrierKindId(0)),
            },
            PressurePlateRuntime {
                level: 0,
                col: 1,
                row: 0,
                purpose: PlatePurpose::Barrier(BarrierKindId(0)),
            },
            PressurePlateRuntime {
                level: 0,
                col: 2,
                row: 0,
                purpose: PlatePurpose::Barrier(BarrierKindId(1)),
            },
        ];
        let holders = HashMap::from([(0, PlayerId(1)), (1, PlayerId(2)), (2, PlayerId(3))]);
        let prev_held = HashSet::from([0]);

        assert_eq!(
            presser_of_kind(BarrierKindId(0), &holders, &prev_held, &plates),
            Some(PlayerId(2))
        );
        assert_eq!(
            presser_of_kind(BarrierKindId(1), &holders, &prev_held, &plates),
            Some(PlayerId(3))
        );
        assert_eq!(presser_of_kind(BarrierKindId(2), &holders, &prev_held, &plates), None);
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
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    use super::*;
    use crate::{
        config::{QuestKind, ServerGameplayConfig},
        map::{CellGrid, EdgeGrid, LevelGrid},
        network::ServerToClient,
        players::PlayerInfo,
        quests::{
            QuestCatalog,
            test_support::{catalog, completed, drain, quest},
        },
    };
    use common::{
        constants::GRID_CELL_SIZE,
        protocol::{QuestId, QuestScope},
    };

    fn firework_plate() -> PressurePlateRuntime {
        PressurePlateRuntime {
            level: 0,
            col: 0,
            row: 0,
            purpose: PlatePurpose::Firework,
        }
    }

    fn app(config: ServerGameplayConfig, plates: Vec<PressurePlateRuntime>) -> App {
        let quest_catalog = QuestCatalog::from_config(&config);
        let board = QuestBoard::from_catalog(&quest_catalog);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(MapConfig {
                levels: vec![LevelGrid {
                    cells: CellGrid::new(2, 2),
                    edges: EdgeGrid::new(2, 2),
                    barrier_edges: EdgeGrid::new(2, 2),
                }],
                actor_spawn_zones: Vec::new(),
                player_spawn_zones: Vec::new(),
                placed_items: Vec::new(),
                pressure_plates: plates,
            })
            .insert_resource(MapGeometry::new(2, 2))
            .insert_resource(PlayerMap::default())
            .insert_resource(config)
            .insert_resource(quest_catalog)
            .insert_resource(board)
            .insert_resource(BarrierKindTable::default())
            .insert_resource(OpenBarrierKinds::default())
            .add_systems(Update, pressure_plates_system);
        app
    }

    // A logged-in player standing in the middle of cell (0, 0).
    fn standing_player(app: &mut App) -> (Entity, UnboundedReceiver<ServerToClient>) {
        let geometry = *app.world().resource::<MapGeometry>();
        let pos = Position {
            x: geometry.cell_to_world_x(0) + GRID_CELL_SIZE / 2.0,
            y: 0.0,
            z: geometry.cell_to_world_z(0) + GRID_CELL_SIZE / 2.0,
        };
        let entity = app.world_mut().spawn((PlayerMarker, PlayerId(1), pos)).id();
        let (tx, mut rx) = unbounded_channel();
        let mut info = PlayerInfo::new(entity, tx);
        info.connection.logged_in = true;
        while rx.try_recv().is_ok() {}
        app.world_mut().resource_mut::<PlayerMap>().insert(PlayerId(1), info);
        (entity, rx)
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
        let (_, mut rx) = standing_player(&mut app);

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
        let (entity, mut rx) = standing_player(&mut app);

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
        let on_plate = *app.world().entity(entity).get::<Position>().expect("position");
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<Position>()
            .expect("position")
            .x += 100.0;
        app.update();
        assert_eq!(clicks(&drain(&mut rx)), 1, "release click");
        *app.world_mut()
            .entity_mut(entity)
            .get_mut::<Position>()
            .expect("position") = on_plate;
        app.update();
        let messages = drain(&mut rx);
        assert_eq!(shows(&messages), 1);
        assert!(!completed(&messages, "show"));
    }
}
