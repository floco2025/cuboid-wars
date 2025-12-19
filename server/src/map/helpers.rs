use rand::{Rng, rngs::ThreadRng};
use std::collections::HashSet;

use crate::resources::GridCell;
use common::{constants::*, protocol::Position};

// Calculate the center position of a grid cell
#[must_use]
pub fn cell_center(grid_x: i32, grid_z: i32) -> Position {
    Position {
        x: (grid_x as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)),
        y: 0.0,
        z: (grid_z as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)),
    }
}

// Convert a world position to grid coordinates
#[must_use]
pub fn grid_coords_from_position(pos: &Position) -> (i32, i32) {
    let grid_x = ((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32;
    let grid_z = ((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32;
    (grid_x, grid_z)
}

// Find a random unoccupied grid cell
#[allow(clippy::implicit_hasher)]
pub fn find_unoccupied_cell(rng: &mut ThreadRng, occupied_cells: &HashSet<(i32, i32)>) -> Option<(i32, i32)> {
    const MAX_ATTEMPTS: usize = 100;
    for _ in 0..MAX_ATTEMPTS {
        let grid_x = rng.random_range(0..GRID_COLS);
        let grid_z = rng.random_range(0..GRID_ROWS);
        if !occupied_cells.contains(&(grid_x, grid_z)) {
            return Some((grid_x, grid_z));
        }
    }
    None
}

// Find an unoccupied cell that's not on a ramp
pub fn find_unoccupied_cell_not_ramp(
    rng: &mut ThreadRng,
    occupied_cells: &HashSet<(i32, i32)>,
    grid: &[Vec<GridCell>],
) -> Option<(i32, i32)> {
    const MAX_ATTEMPTS: usize = 100;
    for _ in 0..MAX_ATTEMPTS {
        let grid_x = rng.random_range(0..GRID_COLS);
        let grid_z = rng.random_range(0..GRID_ROWS);
        if !occupied_cells.contains(&(grid_x, grid_z)) && !grid[grid_z as usize][grid_x as usize].has_ramp {
            return Some((grid_x, grid_z));
        }
    }
    None
}

// Count how many walls a cell has (0-4)
pub(super) const fn count_cell_walls(cell: GridCell) -> u8 {
    let mut count = 0;
    if cell.has_north_wall {
        count += 1;
    }
    if cell.has_south_wall {
        count += 1;
    }
    if cell.has_west_wall {
        count += 1;
    }
    if cell.has_east_wall {
        count += 1;
    }
    count
}
