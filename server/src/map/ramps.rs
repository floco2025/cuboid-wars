use rand::{RngExt, rng};

use crate::{
    constants::{RAMP_COUNT, RAMP_LENGTH_CELLS, RAMP_MIN_SEPARATION_CELLS, RAMP_WIDTH_CELLS},
    resources::GridCell,
};
use common::{constants::*, protocol::Ramp};

pub type Mask = Vec<Vec<bool>>;

// Generate ramps as right triangular prisms whose footprints lie inside `mask`.
// The base sits at `y_high - LEVEL_HEIGHT`; the top reaches `y_high`. `mask` is
// the set of cells where the ramp's lower level has floor coverage.
pub fn generate_ramps_in_mask(
    grid: &mut [Vec<GridCell>],
    mask: &Mask,
    grid_cols: i32,
    grid_rows: i32,
    y_high: f32,
) -> Vec<Ramp> {
    let mut rng = rng();
    let mut ramps = Vec::new();
    let y_low = y_high - LEVEL_HEIGHT;

    if grid_cols < RAMP_LENGTH_CELLS + 2 || grid_rows < RAMP_WIDTH_CELLS + 2 {
        return ramps;
    }

    let mut attempts = 0;
    const MAX_ATTEMPTS: usize = 200;

    while ramps.len() < RAMP_COUNT && attempts < MAX_ATTEMPTS {
        attempts += 1;

        let along_x = rng.random_bool(0.5);

        let (col0, row0, col_end, row_end) = if along_x {
            let c0 = rng.random_range(0..=(grid_cols - RAMP_LENGTH_CELLS));
            let r0 = rng.random_range(0..=(grid_rows - RAMP_WIDTH_CELLS));
            (c0, r0, c0 + RAMP_LENGTH_CELLS, r0 + RAMP_WIDTH_CELLS)
        } else {
            let c0 = rng.random_range(0..=(grid_cols - RAMP_WIDTH_CELLS));
            let r0 = rng.random_range(0..=(grid_rows - RAMP_LENGTH_CELLS));
            (c0, r0, c0 + RAMP_WIDTH_CELLS, r0 + RAMP_LENGTH_CELLS)
        };

        // Each footprint cell must be in mask, and at least one cell must be
        // adjacent to the mask boundary so the ramp's base attaches to an edge.
        let mut footprint_ok = true;
        let mut any_at_boundary = false;
        'check: for col in col0..col_end {
            for row in row0..row_end {
                if !mask[row as usize][col as usize] {
                    footprint_ok = false;
                    break 'check;
                }
                if cell_adjacent_to_boundary(mask, row, col, grid_cols, grid_rows) {
                    any_at_boundary = true;
                }
            }
        }
        if !footprint_ok || !any_at_boundary {
            continue;
        }

        // No overlap with existing ramps in this pass.
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

        // Pick orientation so the high side faces an in-mask cell. Otherwise the
        // ramp would dump the player off the edge of the playable footprint.
        let Some(high_at_end) = pick_high_orientation(
            mask, along_x, col0, col_end, row0, row_end, grid_cols, grid_rows, &mut rng,
        ) else {
            continue;
        };

        for col in col0..col_end {
            for row in row0..row_end {
                grid[row as usize][col as usize].has_ramp = true;
            }
        }

        if along_x {
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
            y1: y_low,
            z1,
            x2,
            y2: y_high,
            z2,
        });
    }

    ramps
}

// True when at least one 4-neighbor is non-mask or out of bounds. A cell that
// satisfies this sits on the boundary of `mask` and is a candidate for a ramp
// base (the side that should drop down to the lower level).
fn cell_adjacent_to_boundary(mask: &Mask, row: i32, col: i32, grid_cols: i32, grid_rows: i32) -> bool {
    for (dr, dc) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        let nr = row + dr;
        let nc = col + dc;
        if nr < 0 || nr >= grid_rows || nc < 0 || nc >= grid_cols {
            return true;
        }
        if !mask[nr as usize][nc as usize] {
            return true;
        }
    }
    false
}

// Returns `Some(high_at_end)` orienting the ramp so its high side faces an
// in-mask cell (where the upper-level floor will be placed). Returns `None`
// when neither orientation lands on the mask, in which case the ramp must be
// rejected.
fn pick_high_orientation(
    mask: &Mask,
    along_x: bool,
    col0: i32,
    col_end: i32,
    row0: i32,
    row_end: i32,
    grid_cols: i32,
    grid_rows: i32,
    rng: &mut impl rand::Rng,
) -> Option<bool> {
    let beyond_in_mask =
        |r: i32, c: i32| -> bool { r >= 0 && r < grid_rows && c >= 0 && c < grid_cols && mask[r as usize][c as usize] };

    let (high_end_ok, high_start_ok) = if along_x {
        let east_ok = (row0..row_end).all(|r| beyond_in_mask(r, col_end));
        let west_ok = (row0..row_end).all(|r| beyond_in_mask(r, col0 - 1));
        (east_ok, west_ok)
    } else {
        let south_ok = (col0..col_end).all(|c| beyond_in_mask(row_end, c));
        let north_ok = (col0..col_end).all(|c| beyond_in_mask(row0 - 1, c));
        (south_ok, north_ok)
    };

    match (high_end_ok, high_start_ok) {
        (true, true) => Some(rng.random_bool(0.5)),
        (true, false) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
    }
}
