use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::resources::{MapConfig, PlayerMap, PressurePlate};
use common::{
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    map_geometry::MapGeometry,
    protocol::{BarrierKindId, PlayerMarker, Position},
};

// World-space test: is `pos` inside this plate's inner 25%-by-area square AND
// on the plate's level? Y matches when `|pos.y - level * LEVEL_HEIGHT| <
// LEVEL_HEIGHT / 2`, which keeps a player on the floor above from triggering
// a plate one level down.
#[must_use]
pub fn player_on_plate(plate: &PressurePlate, pos: &Position, geometry: &MapGeometry) -> bool {
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

// Set of barrier kinds currently held open by pressure plates. Empty when no
// kind's threshold is met. Lives as a resource so the movement system can
// union it into each player's `held_keys` and the broadcast system can ship
// it in `SSnapshot`.
#[derive(Resource, Default, Clone)]
pub struct OpenBarrierKinds(pub Vec<BarrierKindId>);

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
) {
    if map_config.pressure_plates.is_empty() {
        if !open.0.is_empty() {
            open.0.clear();
        }
        return;
    }

    let alive: usize = players
        .iter()
        .filter(|(_, info)| info.logged_in && !info.is_dead())
        .count();

    // For each kind, count total plates and gather the indices of currently-
    // held plates. We dedupe held plates per kind (multiple players on one
    // plate still count once).
    let mut plates_per_kind: HashMap<BarrierKindId, usize> = HashMap::new();
    for plate in &map_config.pressure_plates {
        *plates_per_kind.entry(plate.kind).or_insert(0) += 1;
    }

    let mut held_per_kind: HashMap<BarrierKindId, HashSet<usize>> = HashMap::new();
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
        }
    }

    let mut next = Vec::new();
    for (kind, plates_for_kind) in &plates_per_kind {
        let required = (*plates_for_kind).min(alive.saturating_sub(1));
        let held = held_per_kind.get(kind).map_or(0, HashSet::len);
        if held >= required {
            next.push(*kind);
        }
    }
    next.sort_by_key(|k| k.0);
    if next != open.0 {
        open.0 = next;
    }
}
