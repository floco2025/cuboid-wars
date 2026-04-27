use rand::{RngExt, rngs::ThreadRng};

use crate::constants::{FLOOR_GROUND_DENSITY, FLOOR_SEED_COUNT, FLOOR_TOP_DENSITY};

// A boolean grid covering the whole map; `mask[row][col] == true` means the
// level this mask belongs to has floor at that cell.
pub type Mask = Vec<Vec<bool>>;

// Linearly interpolate floor density between ground and top levels for level k
// of an `num_levels`-tall stack.
#[must_use]
pub fn density_for_level(level: u32, num_levels: u32) -> f32 {
    if num_levels <= 1 {
        return FLOOR_GROUND_DENSITY;
    }
    let t = level as f32 / (num_levels - 1) as f32;
    FLOOR_GROUND_DENSITY + (FLOOR_TOP_DENSITY - FLOOR_GROUND_DENSITY) * t
}

// Grow a fresh mask by picking `FLOOR_SEED_COUNT` random seed cells and
// expanding the frontier (random neighbor of any current frontier cell) until
// the mask hits `target_count` cells or the frontier dries up. Produces 1 to
// `FLOOR_SEED_COUNT` connected regions.
#[must_use]
pub fn generate_mask(rng: &mut ThreadRng, target_count: usize, grid_cols: i32, grid_rows: i32) -> Mask {
    let mut mask = vec![vec![false; grid_cols as usize]; grid_rows as usize];
    let mut frontier: Vec<(i32, i32)> = Vec::new();
    let mut count: usize = 0;

    if target_count == 0 || grid_cols <= 0 || grid_rows <= 0 {
        return mask;
    }

    let max_seeds = FLOOR_SEED_COUNT.min(target_count);
    for _ in 0..max_seeds {
        let r = rng.random_range(0..grid_rows);
        let c = rng.random_range(0..grid_cols);
        if !mask[r as usize][c as usize] {
            mask[r as usize][c as usize] = true;
            frontier.push((r, c));
            count += 1;
            if count >= target_count {
                return mask;
            }
        }
    }

    while count < target_count && !frontier.is_empty() {
        let idx = rng.random_range(0..frontier.len());
        let (r, c) = frontier[idx];

        // Pick a random unfilled 4-neighbor; if none, drop this cell from the frontier.
        let mut options: Vec<(i32, i32)> = Vec::with_capacity(4);
        for (dr, dc) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let nr = r + dr;
            let nc = c + dc;
            if nr < 0 || nr >= grid_rows || nc < 0 || nc >= grid_cols {
                continue;
            }
            if mask[nr as usize][nc as usize] {
                continue;
            }
            options.push((nr, nc));
        }

        if options.is_empty() {
            frontier.swap_remove(idx);
            continue;
        }

        let pick = options[rng.random_range(0..options.len())];
        mask[pick.0 as usize][pick.1 as usize] = true;
        frontier.push(pick);
        count += 1;
    }

    mask
}

// Compute the target floor-cell count for a level given the lerp'd density.
#[must_use]
pub fn target_count_for_level(level: u32, num_levels: u32, grid_cols: i32, grid_rows: i32) -> usize {
    let area = (grid_cols * grid_rows) as f32;
    let density = density_for_level(level, num_levels);
    (area * density).round().max(0.0) as usize
}

// Mark `has_floor_above` on cells of `lower_grid` where `upper_mask[r][c]` is set.
// Used by the wall-lights generator to skip cells that are under a roof.
pub fn mark_has_floor_above(grid: &mut [Vec<crate::resources::GridCell>], upper_mask: &Mask) {
    for (row_idx, row) in grid.iter_mut().enumerate() {
        for (col_idx, cell) in row.iter_mut().enumerate() {
            if upper_mask[row_idx][col_idx] {
                cell.has_floor_above = true;
            }
        }
    }
}

// Mark `has_floor` on cells of `grid` where `mask[r][c]` is set. Used by the
// spawn placer to skip ground holes.
pub fn mark_has_floor(grid: &mut [Vec<crate::resources::GridCell>], mask: &Mask) {
    for (row_idx, row) in grid.iter_mut().enumerate() {
        for (col_idx, cell) in row.iter_mut().enumerate() {
            if mask[row_idx][col_idx] {
                cell.has_floor = true;
            }
        }
    }
}
