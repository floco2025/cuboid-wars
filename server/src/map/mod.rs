mod collision;
mod grid;
mod ramps;
mod roofs;
mod utils;
mod walls;

use rand::Rng;

use crate::{
    constants::{
        MERGE_ROOF_SEGMENTS, MERGE_WALL_SEGMENTS, OVERLAP_ROOFS, OVERLAP_WALLS,
        WALL_NUM_SEGMENTS, WALL_2ND_PROBABILITY_RATIO, WALL_3RD_PROBABILITY_RATIO,
    },
    resources::{GridCell, GridConfig},
};
use common::{
    constants::*,
    protocol::Wall,
};

// Re-export public utilities
pub use utils::{cell_center, grid_coords_from_position, find_unoccupied_cell, find_unoccupied_cell_not_ramp};

/// Generate a complete map grid with walls, roofs, and ramps
#[must_use]
pub fn generate_grid() -> GridConfig {
    let mut rng = rand::rng();

    // Calculate grid dimensions
    let grid_cols = (FIELD_WIDTH / GRID_SIZE) as i32;
    let grid_rows = (FIELD_DEPTH / GRID_SIZE) as i32;

    // Initialize grid with perimeter walls
    let mut grid = vec![vec![GridCell::default(); grid_cols as usize]; grid_rows as usize];

    // Set perimeter walls
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let cell = &mut grid[row as usize][col as usize];

            if row == 0 {
                cell.has_north_wall = true;
            }
            if row == grid_rows - 1 {
                cell.has_south_wall = true;
            }
            if col == 0 {
                cell.has_west_wall = true;
            }
            if col == grid_cols - 1 {
                cell.has_east_wall = true;
            }
        }
    }

    // Generate ramps early so wall placement can respect ramp bases
    let ramps = ramps::generate_ramps(&mut grid, grid_cols, grid_rows);

    // Generate list of all possible interior walls
    // Each wall is represented as (row, col, direction) where direction is: 0=south, 1=east
    let mut possible_walls = Vec::new();

    // Horizontal walls (south edge of cells, except bottom row)
    for row in 0..(grid_rows - 1) {
        for col in 0..grid_cols {
            possible_walls.push((row, col, 0)); // south wall
        }
    }

    // Vertical walls (east edge of cells, except rightmost column)
    for row in 0..grid_rows {
        for col in 0..(grid_cols - 1) {
            possible_walls.push((row, col, 1)); // east wall
        }
    }

    // Shuffle randomly
    for i in (1..possible_walls.len()).rev() {
        let j = rng.random_range(0..=i);
        possible_walls.swap(i, j);
    }

    // Try to place walls
    let mut interior_walls_placed = 0;
    for (row, col, direction) in possible_walls {
        if interior_walls_placed >= WALL_NUM_SEGMENTS {
            break;
        }

        let cell = &grid[row as usize][col as usize];

        // Check if wall is already placed
        let already_has_wall = match direction {
            0 => cell.has_south_wall,
            1 => cell.has_east_wall,
            _ => continue,
        };

        if already_has_wall {
            continue;
        }

        // Disallow walls that would block a ramp base or run through ramp cells
        let ramp_blocked = match direction {
            // south wall between (row,col) and (row+1,col)
            0 => {
                cell.ramp_base_south
                    || cell.ramp_top_south
                    || (row + 1 < grid_rows
                        && (grid[(row + 1) as usize][col as usize].ramp_base_north
                            || grid[(row + 1) as usize][col as usize].ramp_top_north))
                    || (cell.has_ramp && row + 1 < grid_rows && grid[(row + 1) as usize][col as usize].has_ramp)
            }
            // east wall between (row,col) and (row,col+1)
            1 => {
                cell.ramp_base_east
                    || cell.ramp_top_east
                    || (col + 1 < grid_cols
                        && (grid[row as usize][(col + 1) as usize].ramp_base_west
                            || grid[row as usize][(col + 1) as usize].ramp_top_west))
                    || (cell.has_ramp && col + 1 < grid_cols && grid[row as usize][(col + 1) as usize].has_ramp)
            }
            _ => false,
        };
        if ramp_blocked {
            continue;
        }

        // Count existing walls in both cells adjacent to this potential wall
        let cell1_walls = utils::count_cell_walls(*cell);
        let cell2_walls = match direction {
            0 => {
                // South wall - check cell below
                if row < grid_rows - 1 {
                    utils::count_cell_walls(grid[(row + 1) as usize][col as usize])
                } else {
                    0
                }
            }
            1 => {
                // East wall - check cell to the right
                if col < grid_cols - 1 {
                    utils::count_cell_walls(grid[row as usize][(col + 1) as usize])
                } else {
                    0
                }
            }
            _ => 0,
        };

        let max_walls = cell1_walls.max(cell2_walls);

        // Apply probability based on existing wall count
        let ratio = match max_walls {
            0 => 1.0,
            1 => WALL_2ND_PROBABILITY_RATIO,
            _ => WALL_3RD_PROBABILITY_RATIO,
        };

        if ratio < 1.0 && !rng.random_bool(ratio) {
            continue;
        }

        // Temporarily place the wall
        match direction {
            0 => {
                grid[row as usize][col as usize].has_south_wall = true;
                if row < grid_rows - 1 {
                    grid[(row + 1) as usize][col as usize].has_north_wall = true;
                }
            }
            1 => {
                grid[row as usize][col as usize].has_east_wall = true;
                if col < grid_cols - 1 {
                    grid[row as usize][(col + 1) as usize].has_west_wall = true;
                }
            }
            _ => {}
        }

        // Check if all cells are still reachable
        if grid::all_cells_reachable(&grid, grid_cols, grid_rows) {
            interior_walls_placed += 1;
        } else {
            // Remove the wall
            match direction {
                0 => {
                    grid[row as usize][col as usize].has_south_wall = false;
                    if row < grid_rows - 1 {
                        grid[(row + 1) as usize][col as usize].has_north_wall = false;
                    }
                }
                1 => {
                    grid[row as usize][col as usize].has_east_wall = false;
                    if col < grid_cols - 1 {
                        grid[row as usize][(col + 1) as usize].has_west_wall = false;
                    }
                }
                _ => {}
            }
        }
    }

    // Build wall list from grid with individual segments
    let mut walls = walls::generate_individual_walls(&grid, grid_cols, grid_rows);
    if MERGE_WALL_SEGMENTS && !OVERLAP_WALLS {
        walls = walls::merge_walls(walls);
    }

    // Generate roofs based on grid
    let (mut roofs, grid) = roofs::generate_individual_roofs(grid, grid_cols, grid_rows);
    if MERGE_ROOF_SEGMENTS && !OVERLAP_ROOFS {
        roofs = roofs::merge_roofs(roofs);
    }

    // Generate collision walls for roof edges
    let roof_edge_walls = collision::generate_roof_edge_walls(&grid, grid_cols, grid_rows);

    // Separate walls into boundary and interior
    let half_field_width = FIELD_WIDTH / 2.0;
    let half_field_depth = FIELD_DEPTH / 2.0;
    let epsilon = 0.01;

    let (boundary_walls, interior_walls): (Vec<Wall>, Vec<Wall>) = walls.iter().partition(|w| {
        // Check if wall is at the boundary (within epsilon)
        let at_left = (w.x1 + half_field_width).abs() < epsilon && (w.x2 + half_field_width).abs() < epsilon;
        let at_right = (w.x1 - half_field_width).abs() < epsilon && (w.x2 - half_field_width).abs() < epsilon;
        let at_top = (w.z1 + half_field_depth).abs() < epsilon && (w.z2 + half_field_depth).abs() < epsilon;
        let at_bottom = (w.z1 - half_field_depth).abs() < epsilon && (w.z2 - half_field_depth).abs() < epsilon;
        at_left || at_right || at_top || at_bottom
    });

    GridConfig {
        grid,
        boundary_walls,
        interior_walls,
        all_walls: walls,
        roofs,
        ramps,
        roof_edge_walls,
    }
}
