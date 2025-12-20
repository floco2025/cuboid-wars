mod collision;
mod grid;
mod helpers;
mod lights;
mod ramps;
mod roofs;
mod walls;

use rand::Rng;

use crate::constants::{
    ROOF_MERGE_SEGMENTS, WALL_MERGE_SEGMENTS, ROOF_OVERLAP, WALL_OVERLAP, WALL_2ND_PROBABILITY_RATIO,
    WALL_3RD_PROBABILITY_RATIO, WALL_NUM_SEGMENTS,
};
use crate::resources::{GridCell, GridConfig};
use common::{
    constants::*,
    protocol::{MapLayout, Wall},
};
use lights::generate_wall_lights;

// Re-export public utilities
pub use helpers::{cell_center, find_unoccupied_cell, find_unoccupied_cell_not_ramp, grid_coords_from_position};

// Generate a complete map grid with walls, roofs, and ramps
#[must_use]
pub fn generate_grid() -> (MapLayout, GridConfig) {
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
        let cell1_walls = helpers::count_cell_walls(*cell);
        let cell2_walls = match direction {
            0 => {
                // South wall - check cell below
                if row < grid_rows - 1 {
                    helpers::count_cell_walls(grid[(row + 1) as usize][col as usize])
                } else {
                    0
                }
            }
            1 => {
                // East wall - check cell to the right
                if col < grid_cols - 1 {
                    helpers::count_cell_walls(grid[row as usize][(col + 1) as usize])
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
    let mut lower_walls = walls::generate_walls(&grid, grid_cols, grid_rows);
    if WALL_MERGE_SEGMENTS && !WALL_OVERLAP {
        lower_walls = walls::merge_walls(lower_walls);
    }

    // Generate roofs based on grid
    let (mut roofs, grid) = roofs::generate_roofs(grid, grid_cols, grid_rows);
    if ROOF_MERGE_SEGMENTS && !ROOF_OVERLAP {
        roofs = roofs::merge_roofs(roofs);
    }

    // Generate collision walls for roof edges
    let mut roof_walls = collision::generate_roof_walls(&grid, grid_cols, grid_rows);
    if WALL_MERGE_SEGMENTS && !WALL_OVERLAP {
        roof_walls = collision::merge_roof_walls(roof_walls);
    }

    // Separate walls into boundary and interior
    let half_field_width = FIELD_WIDTH / 2.0;
    let half_field_depth = FIELD_DEPTH / 2.0;
    let epsilon = 0.01;

    let (boundary_walls, interior_walls): (Vec<Wall>, Vec<Wall>) = lower_walls.iter().partition(|w| {
        // Check if wall is at the boundary (within epsilon)
        let at_left = (w.x1 + half_field_width).abs() < epsilon && (w.x2 + half_field_width).abs() < epsilon;
        let at_right = (w.x1 - half_field_width).abs() < epsilon && (w.x2 - half_field_width).abs() < epsilon;
        let at_top = (w.z1 + half_field_depth).abs() < epsilon && (w.z2 + half_field_depth).abs() < epsilon;
        let at_bottom = (w.z1 - half_field_depth).abs() < epsilon && (w.z2 - half_field_depth).abs() < epsilon;
        at_left || at_right || at_top || at_bottom
    });

    let wall_lights = generate_wall_lights(&grid);

    let map_layout = MapLayout {
        boundary_walls,
        interior_walls,
        lower_walls,
        roofs,
        ramps,
        roof_walls,
        wall_lights,
    };

    let grid_config = GridConfig { grid };

    (map_layout, grid_config)
}
