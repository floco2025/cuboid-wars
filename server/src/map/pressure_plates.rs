use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::{
    network::broadcast_to_all,
    resources::{MapConfig, PlayerMap, PressurePlateRuntime},
};
use common::{
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    map_geometry::MapGeometry,
    physics::OpenBarrierKinds,
    protocol::{BarrierKindId, PlayerMarker, Position, SPressurePlatePressed, SPressurePlateReleased, ServerMessage},
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

// Per-tick: determine which barrier kinds are open right now.
//
// For each kind that has at least one plate on the map:
//   required = min(plates_for_kind, max(0, alive_logged_in_count - 1))
// If the number of distinct plates currently held by some alive player is
// `>= required`, the kind is open.
//
// Plate is "held" when ≥ 1 alive player is inside the inner 25%-by-area
// square of its cell (see `player_on_plate`).
pub fn compute_open_barrier_kinds_system(
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
    players: Res<PlayerMap>,
    positions: Query<&Position, With<PlayerMarker>>,
    mut open: ResMut<OpenBarrierKinds>,
    // Plate indices held last tick. Used to fire `SPressurePlatePressed`
    // only on the unpressed→pressed edge (step-on cue), not every tick a
    // player keeps standing.
    mut prev_held: Local<HashSet<usize>>,
    // Plate count per kind. Derived from the immutable map, so it is built
    // once on first run rather than rebuilt every tick.
    mut plates_per_kind: Local<HashMap<BarrierKindId, usize>>,
) {
    if map_config.pressure_plates.is_empty() {
        if !open.0.is_empty() {
            open.0.clear();
        }
        prev_held.clear();
        return;
    }

    if plates_per_kind.is_empty() {
        for plate in &map_config.pressure_plates {
            *plates_per_kind.entry(plate.kind).or_insert(0) += 1;
        }
    }

    let alive: usize = players
        .iter()
        .filter(|(_, info)| info.logged_in && !info.is_dead())
        .count();

    // Per-tick: which plate indices are held, grouped per kind for the
    // open-set computation and flat for edge-trigger detection.
    let mut held_per_kind: HashMap<BarrierKindId, HashSet<usize>> = HashMap::new();
    let mut held_indices: HashSet<usize> = HashSet::new();
    for (idx, plate) in map_config.pressure_plates.iter().enumerate() {
        let mut held = false;
        for (_, info) in players.iter() {
            if !info.logged_in || info.is_dead() {
                continue;
            }
            let Ok(pos) = positions.get(info.entity) else {
                continue;
            };
            if player_on_plate(plate, pos, &map_geometry) {
                held = true;
                break;
            }
        }
        if held {
            held_per_kind.entry(plate.kind).or_default().insert(idx);
            held_indices.insert(idx);
        }
    }

    // Edge-triggered cues: at most one press and one release cue per tick,
    // regardless of how many plates flipped — the messages carry no plate
    // identity, so collapsing simultaneous flips is lossless. Persistent state
    // lives in `OpenBarrierKinds` + snapshot; these are pure click/clunk SFX.
    if held_indices.difference(&prev_held).next().is_some() {
        broadcast_to_all(&players, ServerMessage::PressurePlatePressed(SPressurePlatePressed {}));
    }
    if prev_held.difference(&held_indices).next().is_some() {
        broadcast_to_all(
            &players,
            ServerMessage::PressurePlateReleased(SPressurePlateReleased {}),
        );
    }
    *prev_held = held_indices;

    let mut next = Vec::new();
    for (kind, plates_for_kind) in plates_per_kind.iter() {
        let required = (*plates_for_kind).min(alive.saturating_sub(1));
        let held = held_per_kind.get(kind).map_or(0, HashSet::len);
        if held >= required {
            next.push(*kind);
        }
    }
    // Stable order for the equality diff below — without it, the HashMap
    // iteration order varies tick-to-tick and we'd rewrite the resource
    // every tick (defeating change detection on both server broadcast and
    // client visibility).
    next.sort_by_key(|k| k.0);
    if next != open.0 {
        open.0 = next;
    }
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
            kind: BarrierKindId(0),
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
