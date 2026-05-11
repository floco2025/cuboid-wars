use rand::{RngExt, rngs::ThreadRng};

use crate::{
    constants::{ITEM_CELLS_PER_ACTIVE, ITEM_LIFETIME, ITEM_MAX_ACTIVE, ITEM_MIN_ACTIVE},
    map::grid_coords_from_position,
    resources::MapConfig,
};
use common::{
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    map::compute_player_level,
    map_geometry::MapGeometry,
    protocol::{ItemType, Position},
};

pub(super) fn choose_item_type(rng: &mut ThreadRng) -> ItemType {
    // Uniform 1/4 over the four power-up variants. `Cookie` is spawned
    // separately and isn't in this pool.
    let rand_val = rng.random::<f64>();
    if rand_val < 1.0 / 4.0 {
        ItemType::SpeedPowerUp
    } else if rand_val < 2.0 / 4.0 {
        ItemType::MultiShotPowerUp
    } else if rand_val < 3.0 / 4.0 {
        ItemType::PhasingPowerUp
    } else {
        ItemType::AntiGravityPowerUp
    }
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
    if eligible_cell_count == 0 {
        return 0;
    }
    eligible_cell_count
        .div_ceil(ITEM_CELLS_PER_ACTIVE)
        .clamp(ITEM_MIN_ACTIVE, ITEM_MAX_ACTIVE)
        .min(eligible_cell_count)
        .max(1)
}

pub(super) fn power_up_spawn_interval(eligible_cell_count: usize) -> Option<f32> {
    let target_active = target_active_power_ups(eligible_cell_count);
    (target_active > 0).then_some(ITEM_LIFETIME / target_active as f32)
}
