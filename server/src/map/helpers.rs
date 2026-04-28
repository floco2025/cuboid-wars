use rand::{RngExt, rngs::ThreadRng};
use std::collections::HashSet;

use super::edges::{CellSide, has_edge_on_cell_side};
use crate::resources::{CellGrid, EdgeGrid};
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

// Find an unoccupied level-0 floor cell that isn't on a ramp.
pub fn find_unoccupied_cell_not_ramp(
    rng: &mut ThreadRng,
    occupied_cells: &HashSet<(i32, i32)>,
    cells: &CellGrid,
) -> Option<(i32, i32)> {
    const MAX_ATTEMPTS: usize = 100;
    for _ in 0..MAX_ATTEMPTS {
        let grid_x = rng.random_range(0..GRID_COLS);
        let grid_z = rng.random_range(0..GRID_ROWS);
        let cell = cells.rows[grid_z as usize][grid_x as usize];
        if !occupied_cells.contains(&(grid_x, grid_z)) && cell.has_floor && !cell.has_ramp {
            return Some((grid_x, grid_z));
        }
    }
    None
}

// Count how many cell sides have edges (0-4).
#[allow(dead_code)]
pub(super) fn count_cell_side_edges(edges: &EdgeGrid, row: i32, col: i32) -> u8 {
    [CellSide::North, CellSide::South, CellSide::West, CellSide::East]
        .into_iter()
        .filter(|side| has_edge_on_cell_side(edges, row, col, *side))
        .count()
        .try_into()
        .expect("a cell has at most four edges")
}
