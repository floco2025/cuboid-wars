use rand::{RngExt, rngs::ThreadRng};

use crate::{
    constants::{ITEM_LIFETIME, ITEM_TARGET_ACTIVE},
    map::grid_coords_from_position,
    resources::MapConfig,
};
use common::{
    constants::{
        GRID_CELL_SIZE, LEVEL_HEIGHT, POWER_UP_ANTI_GRAVITY_ENABLED, POWER_UP_MULTI_SHOT_ENABLED,
        POWER_UP_PHASING_ENABLED, POWER_UP_SPEED_ENABLED,
    },
    map::compute_player_level,
    map_geometry::MapGeometry,
    protocol::{ItemType, Position},
};

// Uniform pick over the variants enabled by the `POWER_UP_*_ENABLED` flags
// in `common::constants`. `Cookie` is spawned separately and isn't in this
// pool. Returns `None` when every power-up is disabled — the caller skips
// the spawn entirely so disabling everything leaves a cookie-only map.
pub(super) fn choose_item_type(rng: &mut ThreadRng) -> Option<ItemType> {
    let pool: [(bool, ItemType); 4] = [
        (POWER_UP_SPEED_ENABLED, ItemType::SpeedPowerUp),
        (POWER_UP_MULTI_SHOT_ENABLED, ItemType::MultiShotPowerUp),
        (POWER_UP_PHASING_ENABLED, ItemType::PhasingPowerUp),
        (POWER_UP_ANTI_GRAVITY_ENABLED, ItemType::AntiGravityPowerUp),
    ];
    let enabled: Vec<ItemType> = pool.into_iter().filter_map(|(e, t)| e.then_some(t)).collect();
    if enabled.is_empty() {
        return None;
    }
    Some(enabled[rng.random_range(0..enabled.len())])
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ItemSpawnCell {
    pub(super) level: u8,
    pub(super) col: i32,
    pub(super) row: i32,
}

impl ItemSpawnCell {
    pub(super) fn position(self, geometry: &MapGeometry) -> Position {
        Position {
            x: geometry.cell_to_world_x(self.col) + GRID_CELL_SIZE / 2.0,
            y: f32::from(self.level) * LEVEL_HEIGHT,
            z: geometry.cell_to_world_z(self.row) + GRID_CELL_SIZE / 2.0,
        }
    }
}

pub(super) fn item_spawn_cell_from_position(geometry: &MapGeometry, pos: &Position) -> ItemSpawnCell {
    let (col, row) = grid_coords_from_position(geometry, pos);
    ItemSpawnCell {
        level: compute_player_level(pos.y),
        col,
        row,
    }
}

pub(super) fn eligible_item_spawn_cells(map_config: &MapConfig) -> Vec<ItemSpawnCell> {
    let mut cells = Vec::new();
    for (level_idx, level_grid) in map_config.levels.iter().enumerate() {
        let level = u8::try_from(level_idx).unwrap_or(u8::MAX);
        for (row, grid_row) in level_grid.cells.rows.iter().enumerate() {
            for (col, cell) in grid_row.iter().enumerate() {
                if cell.has_floor && !cell.has_ramp {
                    cells.push(ItemSpawnCell {
                        level,
                        col: col as i32,
                        row: row as i32,
                    });
                }
            }
        }
    }
    cells
}

pub(super) fn target_active_power_ups(eligible_cell_count: usize) -> usize {
    // Constant target regardless of map size, capped at what the map can
    // actually hold so tiny test maps don't try to spawn more items than
    // there are floor cells.
    ITEM_TARGET_ACTIVE.min(eligible_cell_count)
}

pub(super) fn power_up_spawn_interval(eligible_cell_count: usize) -> Option<f32> {
    let target_active = target_active_power_ups(eligible_cell_count);
    (target_active > 0).then_some(ITEM_LIFETIME / target_active as f32)
}
