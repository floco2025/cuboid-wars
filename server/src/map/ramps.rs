use rand::Rng;

use crate::{
    constants::{RAMP_COUNT, RAMP_LENGTH_CELLS, RAMP_MIN_SEPARATION_CELLS, RAMP_WIDTH_CELLS},
    resources::GridCell,
};
use common::{constants::*, protocol::Ramp};

// Generate ramps as right triangular prisms using opposite corners
pub fn generate_ramps(grid: &mut [Vec<GridCell>], grid_cols: i32, grid_rows: i32) -> Vec<Ramp> {
    let mut rng = rand::rng();
    let mut ramps = Vec::new();

    if grid_cols < RAMP_LENGTH_CELLS + 2 || grid_rows < RAMP_WIDTH_CELLS + 2 {
        return ramps;
    }

    // Try placing ramps until we reach target count
    let mut attempts = 0;
    const MAX_ATTEMPTS: usize = 100;

    while ramps.len() < RAMP_COUNT && attempts < MAX_ATTEMPTS {
        attempts += 1;

        // Random orientation: true = along X axis (west-east), false = along Z axis (north-south)
        let along_x = rng.random_bool(0.5);

        // Random position
        let (col0, row0, col_end, row_end) = if along_x {
            let c0 = rng.random_range(1..=(grid_cols - RAMP_LENGTH_CELLS - 1));
            let r0 = rng.random_range(1..=(grid_rows - RAMP_WIDTH_CELLS - 1));
            (c0, r0, c0 + RAMP_LENGTH_CELLS, r0 + RAMP_WIDTH_CELLS)
        } else {
            let c0 = rng.random_range(1..=(grid_cols - RAMP_WIDTH_CELLS - 1));
            let r0 = rng.random_range(1..=(grid_rows - RAMP_LENGTH_CELLS - 1));
            (c0, r0, c0 + RAMP_WIDTH_CELLS, r0 + RAMP_LENGTH_CELLS)
        };

        // Check if all cells in ramp footprint are in allowed zone (1-2 cells from any border)
        let mut in_allowed_zone = true;
        'zone_check: for col in col0..col_end {
            for row in row0..row_end {
                // Distance from border walls (cell 0 and cell grid_cols-1 are border walls)
                let dist_to_west = col;
                let dist_to_east = (grid_cols - 1) - col;
                let dist_to_north = row;
                let dist_to_south = (grid_rows - 1) - row;
                let min_dist = dist_to_west.min(dist_to_east).min(dist_to_north).min(dist_to_south);

                // Allowed zone: cells must be 1 or 2 cells from border
                if min_dist < 1 || min_dist > 2 {
                    in_allowed_zone = false;
                    break 'zone_check;
                }
            }
        }
        if !in_allowed_zone {
            continue;
        }

        // Check for overlaps with existing ramps (including separation padding)
        let pad = RAMP_MIN_SEPARATION_CELLS;
        let mut overlaps = false;
        for col in (col0 - pad).max(0)..(col_end + pad).min(grid_cols) {
            for row in (row0 - pad).max(0)..(row_end + pad).min(grid_rows) {
                if grid[row as usize][col as usize].has_ramp {
                    overlaps = true;
                    break;
                }
            }
            if overlaps {
                break;
            }
        }
        if overlaps {
            continue;
        }

        // Mark all cells in footprint
        for col in col0..col_end {
            for row in row0..row_end {
                grid[row as usize][col as usize].has_ramp = true;
            }
        }

        // Randomly decide which end is elevated
        let high_at_end = rng.random_bool(0.5);

        // Mark base (low) and top (high) edge flags
        if along_x {
            // Ramp along X: choose which end (west or east) is high
            for row in row0..row_end {
                if high_at_end {
                    grid[row as usize][col0 as usize].ramp_base_west = true;
                    grid[row as usize][(col_end - 1) as usize].ramp_top_east = true;
                } else {
                    grid[row as usize][(col_end - 1) as usize].ramp_base_east = true;
                    grid[row as usize][col0 as usize].ramp_top_west = true;
                }
            }
        } else {
            // Ramp along Z: choose which end (north or south) is high
            for col in col0..col_end {
                if high_at_end {
                    grid[row0 as usize][col as usize].ramp_base_north = true;
                    grid[(row_end - 1) as usize][col as usize].ramp_top_south = true;
                } else {
                    grid[(row_end - 1) as usize][col as usize].ramp_base_south = true;
                    grid[row0 as usize][col as usize].ramp_top_north = true;
                }
            }
        }

        // Create Ramp: (x1,y1,z1) = low corner, (x2,y2,z2) = high corner
        let x_start = (col0 as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let z_start = (row0 as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
        let x_end = (col_end as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let z_end = (row_end as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

        let (x1, z1, x2, z2) = if high_at_end {
            (x_start, z_start, x_end, z_end)
        } else {
            (x_end, z_end, x_start, z_start)
        };

        ramps.push(Ramp {
            x1,
            y1: 0.0,
            z1,
            x2,
            y2: WALL_HEIGHT + ROOF_THICKNESS, // Ramp top goes to top of roof
            z2,
        });
    }

    ramps
}
