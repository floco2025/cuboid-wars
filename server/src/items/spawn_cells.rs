use rand::{Rng, seq::IndexedRandom};

use crate::map::{CarrierGrid, grid_coords_from_position};
use common::{
    map::MapGeometry,
    protocol::{ItemType, Position},
};

pub(super) fn choose_item_type(rng: &mut impl Rng, pool: &[(ItemType, f64)]) -> Option<ItemType> {
    if pool.is_empty() {
        return None;
    }
    Some(
        pool.choose_weighted(rng, |(_, weight)| *weight)
            .expect("random item weights are invalid after config validation")
            .0,
    )
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
            x: geometry.cell_center_x(self.col),
            y: geometry.level_y(self.level),
            z: geometry.cell_center_z(self.row),
        }
    }
}

pub(super) fn item_spawn_cell_from_position(geometry: &MapGeometry, pos: &Position) -> ItemSpawnCell {
    let (col, row) = grid_coords_from_position(geometry, pos);
    ItemSpawnCell {
        level: geometry.level_for_y(pos.y),
        col,
        row,
    }
}

pub(super) fn eligible_item_spawn_cells(grid: &CarrierGrid) -> Vec<ItemSpawnCell> {
    let mut cells = Vec::new();
    for (level_idx, level_grid) in grid.levels.iter().enumerate() {
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

// Configured target capped at what the map can actually hold so tiny test
// maps don't try to spawn more items than there are floor cells.
pub(super) fn target_active_random_items(eligible_cell_count: usize, max_number: usize) -> usize {
    max_number.min(eligible_cell_count)
}
