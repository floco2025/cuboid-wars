mod floors;
mod grid;
mod helpers;
mod lights;
mod ramps;
mod walls;

use rand::{RngExt, rng};

use crate::{
    constants::{
        FLOOR_MERGE_SEGMENTS, FLOOR_OVERLAP, WALL_2ND_PROBABILITY_RATIO, WALL_3RD_PROBABILITY_RATIO,
        WALL_MERGE_SEGMENTS, WALL_NUM_SEGMENTS, WALL_OVERLAP,
    },
    resources::{GridCell, GridConfig},
};
use common::{
    constants::*,
    protocol::{Floor, MapLayout},
};
use lights::generate_wall_lights;

pub use helpers::{cell_center, find_unoccupied_cell, find_unoccupied_cell_not_ramp, grid_coords_from_position};

// Generate a complete map grid with walls, floors, and ramps for `num_levels` levels.
#[must_use]
pub fn generate_grid(num_levels: u32) -> (MapLayout, GridConfig) {
    let num_levels = num_levels.max(1);
    let mut rng = rng();

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

    let mut level0_walls = walls::generate_lower_walls(&grid, grid_cols, grid_rows);
    if WALL_MERGE_SEGMENTS && !WALL_OVERLAP {
        level0_walls = walls::merge_walls(level0_walls);
    }

    let (mut level1_floors, grid) = floors::generate_level1_floors(grid, grid_cols, grid_rows);
    if FLOOR_MERGE_SEGMENTS && !FLOOR_OVERLAP {
        level1_floors = floors::merge_floors(level1_floors);
    }

    let wall_lights = generate_wall_lights(&grid);

    let half_w = FIELD_WIDTH / 2.0;
    let half_d = FIELD_DEPTH / 2.0;
    let mut floors = Vec::with_capacity(1 + level1_floors.len() * (num_levels.saturating_sub(1) as usize));
    floors.push(Floor {
        x1: -half_w,
        z1: -half_d,
        x2: half_w,
        z2: half_d,
        y: 0.0,
        thickness: 0.0,
        level: 0,
    });
    for level in 1..num_levels {
        let level_y = LEVEL_HEIGHT * level as f32;
        let level_u8 = u8::try_from(level).unwrap_or(u8::MAX);
        for f in &level1_floors {
            floors.push(Floor {
                x1: f.x1,
                z1: f.z1,
                x2: f.x2,
                z2: f.z2,
                y: level_y,
                thickness: f.thickness,
                level: level_u8,
            });
        }
    }

    let map_layout = MapLayout {
        walls: level0_walls,
        ramps,
        wall_lights,
        floors,
    };

    let grid_config = GridConfig { grid };

    (map_layout, grid_config)
}
