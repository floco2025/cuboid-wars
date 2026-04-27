mod floors;
mod grid;
mod helpers;
mod lights;
mod ramps;
mod walls;

use rand::{RngExt, rng, rngs::ThreadRng};

use crate::{
    constants::{
        FLOOR_MERGE_SEGMENTS, FLOOR_NUM_SEGMENTS, FLOOR_OVERLAP, WALL_2ND_PROBABILITY_RATIO,
        WALL_3RD_PROBABILITY_RATIO, WALL_MERGE_SEGMENTS, WALL_NUM_SEGMENTS, WALL_OVERLAP,
    },
    resources::{GridCell, GridConfig},
};
use common::{
    constants::*,
    protocol::{Floor, MapLayout, Ramp, Wall},
};
use lights::generate_wall_lights;
use ramps::Mask;

pub use helpers::{cell_center, find_unoccupied_cell, find_unoccupied_cell_not_ramp, grid_coords_from_position};

// Generate a complete map with walls, floors, and ramps for `num_levels` levels.
//
// Levels are generated bottom-up. Each iteration produces:
// - the current level's walls
// - ramps from the current level up to the next
// - the next level's floor cells (subset of the current level's footprint)
//
// `GridConfig` carries the level-0 grid (used by item/player spawn).
#[must_use]
pub fn generate_grid(num_levels: u32) -> (MapLayout, GridConfig) {
    let num_levels = num_levels.max(1);
    let mut rng = rng();

    let grid_cols = (FIELD_WIDTH / GRID_SIZE) as i32;
    let grid_rows = (FIELD_DEPTH / GRID_SIZE) as i32;

    let mut all_walls: Vec<Wall> = Vec::new();
    let mut all_ramps: Vec<Ramp> = Vec::new();
    let mut all_floors: Vec<Floor> = Vec::new();

    // Level-0 ground floor
    let half_w = FIELD_WIDTH / 2.0;
    let half_d = FIELD_DEPTH / 2.0;
    all_floors.push(Floor {
        x1: -half_w,
        z1: -half_d,
        x2: half_w,
        z2: half_d,
        y: 0.0,
        thickness: 0.0,
        level: 0,
    });

    // Mask of cells that have floor at the level we're about to build *up from*.
    let mut prev_mask: Mask = vec![vec![true; grid_cols as usize]; grid_rows as usize];
    let mut level0_grid: Option<Vec<Vec<GridCell>>> = None;

    let max_level = u8::try_from(num_levels.saturating_sub(1)).unwrap_or(u8::MAX);

    // For each level pair k -> k+1, generate ramps + walls at level k and floors at k+1.
    for level in 0..max_level {
        let next_level = level + 1;
        let next_y = f32::from(next_level) * LEVEL_HEIGHT;

        let mut grid = build_level_grid(&prev_mask, grid_cols, grid_rows);

        let ramps = ramps::generate_ramps_in_mask(&mut grid, &prev_mask, grid_cols, grid_rows, next_y);

        place_interior_walls(&mut grid, &prev_mask, grid_cols, grid_rows, &mut rng);

        let (mut floors_at_next, next_mask) = floors::generate_floor_tier(
            &mut grid,
            &prev_mask,
            grid_cols,
            grid_rows,
            next_level,
            next_y,
            FLOOR_NUM_SEGMENTS,
        );
        if FLOOR_MERGE_SEGMENTS && !FLOOR_OVERLAP {
            floors_at_next = floors::merge_floors(floors_at_next);
        }

        let mut walls_at_level = walls::generate_walls(&grid, grid_cols, grid_rows, level);
        if WALL_MERGE_SEGMENTS && !WALL_OVERLAP {
            walls_at_level = walls::merge_walls(walls_at_level);
        }

        all_walls.extend(walls_at_level);
        all_ramps.extend(ramps);
        all_floors.extend(floors_at_next);

        if level == 0 {
            level0_grid = Some(grid);
        }
        prev_mask = next_mask;
    }

    let grid = level0_grid.unwrap_or_else(|| build_level_grid(&prev_mask, grid_cols, grid_rows));
    let wall_lights = generate_wall_lights(&grid);

    let map_layout = MapLayout {
        walls: all_walls,
        ramps: all_ramps,
        wall_lights,
        floors: all_floors,
    };

    (map_layout, GridConfig { grid })
}

// Build a fresh grid with perimeter walls on the in-mask side of every mask
// boundary edge. Out-of-mask cells stay default — wall emission walks
// `has_*_wall` flags, so leaving them clean keeps walls confined to the
// playable footprint.
fn build_level_grid(mask: &Mask, grid_cols: i32, grid_rows: i32) -> Vec<Vec<GridCell>> {
    let mut grid = vec![vec![GridCell::default(); grid_cols as usize]; grid_rows as usize];

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            if !mask[row as usize][col as usize] {
                continue;
            }
            let cell = &mut grid[row as usize][col as usize];
            if row == 0 || !mask[(row - 1) as usize][col as usize] {
                cell.has_north_wall = true;
            }
            if row == grid_rows - 1 || !mask[(row + 1) as usize][col as usize] {
                cell.has_south_wall = true;
            }
            if col == 0 || !mask[row as usize][(col - 1) as usize] {
                cell.has_west_wall = true;
            }
            if col == grid_cols - 1 || !mask[row as usize][(col + 1) as usize] {
                cell.has_east_wall = true;
            }
        }
    }

    grid
}

// Place interior walls inside `mask` cells using the maze-generator algorithm
// (random shuffle, wall-density probability ratios, reachability check).
fn place_interior_walls(grid: &mut [Vec<GridCell>], mask: &Mask, grid_cols: i32, grid_rows: i32, rng: &mut ThreadRng) {
    let mut possible_walls = Vec::new();

    for row in 0..(grid_rows - 1) {
        for col in 0..grid_cols {
            if mask[row as usize][col as usize] && mask[(row + 1) as usize][col as usize] {
                possible_walls.push((row, col, 0)); // south wall
            }
        }
    }

    for row in 0..grid_rows {
        for col in 0..(grid_cols - 1) {
            if mask[row as usize][col as usize] && mask[row as usize][(col + 1) as usize] {
                possible_walls.push((row, col, 1)); // east wall
            }
        }
    }

    for i in (1..possible_walls.len()).rev() {
        let j = rng.random_range(0..=i);
        possible_walls.swap(i, j);
    }

    let mut placed = 0;
    for (row, col, direction) in possible_walls {
        if placed >= WALL_NUM_SEGMENTS {
            break;
        }

        let cell = &grid[row as usize][col as usize];

        let already_has_wall = match direction {
            0 => cell.has_south_wall,
            1 => cell.has_east_wall,
            _ => continue,
        };
        if already_has_wall {
            continue;
        }

        let ramp_blocked = match direction {
            0 => {
                cell.ramp_base_south
                    || cell.ramp_top_south
                    || (row + 1 < grid_rows
                        && (grid[(row + 1) as usize][col as usize].ramp_base_north
                            || grid[(row + 1) as usize][col as usize].ramp_top_north))
                    || (cell.has_ramp && row + 1 < grid_rows && grid[(row + 1) as usize][col as usize].has_ramp)
            }
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

        let cell1_walls = helpers::count_cell_walls(*cell);
        let cell2_walls = match direction {
            0 if row < grid_rows - 1 => helpers::count_cell_walls(grid[(row + 1) as usize][col as usize]),
            1 if col < grid_cols - 1 => helpers::count_cell_walls(grid[row as usize][(col + 1) as usize]),
            _ => 0,
        };
        let max_walls = cell1_walls.max(cell2_walls);

        let ratio = match max_walls {
            0 => 1.0,
            1 => WALL_2ND_PROBABILITY_RATIO,
            _ => WALL_3RD_PROBABILITY_RATIO,
        };
        if ratio < 1.0 && !rng.random_bool(ratio) {
            continue;
        }

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

        if grid::all_cells_reachable_within_mask(grid, mask, grid_cols, grid_rows) {
            placed += 1;
        } else {
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
}
