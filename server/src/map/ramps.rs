use rand::{RngExt, rngs::ThreadRng};
use std::collections::{HashSet, VecDeque};

use super::mask::Mask;
use crate::{
    constants::{RAMP_LENGTH_CELLS, RAMP_MIN_SEPARATION_CELLS, RAMP_WIDTH_CELLS},
    resources::GridCell,
};
use common::{constants::*, protocol::Ramp};

// Internal representation of a ramp candidate. Carries enough cell-level
// information to drive the cross-level BFS without round-tripping through
// world coordinates.
#[derive(Debug, Clone)]
pub struct RampSpec {
    pub lower_level: u32,
    pub along_x: bool,
    pub high_at_end: bool,
    pub col0: i32,
    pub row0: i32,
    pub col_end: i32,
    pub row_end: i32,
}

impl RampSpec {
    fn footprint_cells(&self) -> Vec<(i32, i32)> {
        let mut cells = Vec::new();
        for col in self.col0..self.col_end {
            for row in self.row0..self.row_end {
                cells.push((row, col));
            }
        }
        cells
    }

    // Cells at the lower level adjacent to the base side of the footprint.
    // Players entering the ramp walk in from one of these.
    fn lower_entry_cells(&self) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        if self.along_x {
            // Base is at col0 if high_at_end; at col_end-1 otherwise.
            if self.high_at_end {
                for row in self.row0..self.row_end {
                    out.push((row, self.col0 - 1));
                }
            } else {
                for row in self.row0..self.row_end {
                    out.push((row, self.col_end));
                }
            }
        } else if self.high_at_end {
            for col in self.col0..self.col_end {
                out.push((self.row0 - 1, col));
            }
        } else {
            for col in self.col0..self.col_end {
                out.push((self.row_end, col));
            }
        }
        out
    }

    // Cells at the upper level adjacent to the top side of the footprint.
    // Players exiting the ramp walk out onto one of these.
    fn upper_exit_cells(&self) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        if self.along_x {
            if self.high_at_end {
                for row in self.row0..self.row_end {
                    out.push((row, self.col_end));
                }
            } else {
                for row in self.row0..self.row_end {
                    out.push((row, self.col0 - 1));
                }
            }
        } else if self.high_at_end {
            for col in self.col0..self.col_end {
                out.push((self.row_end, col));
            }
        } else {
            for col in self.col0..self.col_end {
                out.push((self.row0 - 1, col));
            }
        }
        out
    }

    fn to_ramp(&self) -> Ramp {
        let y_low = self.lower_level as f32 * LEVEL_HEIGHT;
        let y_high = (self.lower_level + 1) as f32 * LEVEL_HEIGHT;

        let x_start = (self.col0 as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let z_start = (self.row0 as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
        let x_end = (self.col_end as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let z_end = (self.row_end as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

        let (x1, z1, x2, z2) = if self.high_at_end {
            (x_start, z_start, x_end, z_end)
        } else {
            (x_end, z_end, x_start, z_start)
        };

        Ramp {
            x1,
            y1: y_low,
            z1,
            x2,
            y2: y_high,
            z2,
        }
    }
}

// Enumerate every valid ramp placement: footprint inside the lower level's
// mask, top-side neighbor in the upper level's mask. Both orientations of an
// otherwise-identical footprint are emitted as separate candidates.
pub fn enumerate_candidates(masks: &[Mask], grid_cols: i32, grid_rows: i32) -> Vec<RampSpec> {
    let mut candidates = Vec::new();
    if grid_cols < RAMP_LENGTH_CELLS + 2 || grid_rows < RAMP_WIDTH_CELLS + 2 {
        return candidates;
    }

    for lower_level in 0..masks.len().saturating_sub(1) {
        let lower = &masks[lower_level];
        let upper = &masks[lower_level + 1];

        for along_x in [true, false] {
            let (max_c0, max_r0, len_c, len_r) = if along_x {
                (
                    grid_cols - RAMP_LENGTH_CELLS,
                    grid_rows - RAMP_WIDTH_CELLS,
                    RAMP_LENGTH_CELLS,
                    RAMP_WIDTH_CELLS,
                )
            } else {
                (
                    grid_cols - RAMP_WIDTH_CELLS,
                    grid_rows - RAMP_LENGTH_CELLS,
                    RAMP_WIDTH_CELLS,
                    RAMP_LENGTH_CELLS,
                )
            };

            for c0 in 0..=max_c0 {
                for r0 in 0..=max_r0 {
                    let col_end = c0 + len_c;
                    let row_end = r0 + len_r;

                    if !footprint_in_mask(lower, c0, col_end, r0, row_end) {
                        continue;
                    }

                    for high_at_end in [true, false] {
                        let spec = RampSpec {
                            lower_level: lower_level as u32,
                            along_x,
                            high_at_end,
                            col0: c0,
                            row0: r0,
                            col_end,
                            row_end,
                        };

                        if !exits_lie_in_upper(&spec, upper, grid_cols, grid_rows) {
                            continue;
                        }

                        candidates.push(spec);
                    }
                }
            }
        }
    }

    candidates
}

fn footprint_in_mask(mask: &Mask, col0: i32, col_end: i32, row0: i32, row_end: i32) -> bool {
    for col in col0..col_end {
        for row in row0..row_end {
            if !mask[row as usize][col as usize] {
                return false;
            }
        }
    }
    true
}

fn exits_lie_in_upper(spec: &RampSpec, upper: &Mask, grid_cols: i32, grid_rows: i32) -> bool {
    let in_upper = |r: i32, c: i32| -> bool {
        r >= 0 && r < grid_rows && c >= 0 && c < grid_cols && upper[r as usize][c as usize]
    };
    spec.upper_exit_cells().iter().all(|&(r, c)| in_upper(r, c))
}

// Greedy connectivity-driven placement. Starts from a level-0 in-mask cell and
// keeps adding the candidate that grows the reachable set the most, until the
// world is fully reachable or no candidate makes progress. Candidates that
// would conflict with an already-accepted ramp's footprint (with separation
// padding) are skipped. Ties on gain are broken at random.
pub fn place_connecting_ramps(rng: &mut ThreadRng, masks: &[Mask], grid_cols: i32, grid_rows: i32) -> Vec<RampSpec> {
    let candidates = enumerate_candidates(masks, grid_cols, grid_rows);
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut accepted: Vec<RampSpec> = Vec::new();
    let mut occupied: HashSet<(i32, i32, u32)> = HashSet::new();
    let total_in_mask = count_in_mask_cells(masks);

    loop {
        let reach = bfs_reachability(masks, &accepted, grid_cols, grid_rows);
        if reach.len() >= total_in_mask {
            break;
        }

        // For each available candidate, compute the gain in reachability.
        let mut best_gain: usize = 0;
        let mut best_pool: Vec<RampSpec> = Vec::new();

        for cand in &candidates {
            if conflicts_with_occupied(cand, &occupied) {
                continue;
            }
            let mut trial = accepted.clone();
            trial.push(cand.clone());
            let new_reach = bfs_reachability(masks, &trial, grid_cols, grid_rows);
            let gain = new_reach.len().saturating_sub(reach.len());
            if gain == 0 {
                continue;
            }
            if gain > best_gain {
                best_gain = gain;
                best_pool.clear();
                best_pool.push(cand.clone());
            } else if gain == best_gain {
                best_pool.push(cand.clone());
            }
        }

        if best_pool.is_empty() {
            break;
        }

        let chosen = best_pool[rng.random_range(0..best_pool.len())].clone();
        for (r, c) in footprint_with_pad(&chosen, grid_cols, grid_rows) {
            occupied.insert((r, c, chosen.lower_level));
        }
        accepted.push(chosen);
    }

    accepted
}

fn footprint_with_pad(spec: &RampSpec, grid_cols: i32, grid_rows: i32) -> Vec<(i32, i32)> {
    let pad = RAMP_MIN_SEPARATION_CELLS;
    let mut out = Vec::new();
    for col in (spec.col0 - pad).max(0)..(spec.col_end + pad).min(grid_cols) {
        for row in (spec.row0 - pad).max(0)..(spec.row_end + pad).min(grid_rows) {
            out.push((row, col));
        }
    }
    out
}

fn conflicts_with_occupied(spec: &RampSpec, occupied: &HashSet<(i32, i32, u32)>) -> bool {
    for (row, col) in spec.footprint_cells() {
        if occupied.contains(&(row, col, spec.lower_level)) {
            return true;
        }
    }
    false
}

fn count_in_mask_cells(masks: &[Mask]) -> usize {
    masks.iter().flatten().flatten().filter(|&&v| v).count()
}

// BFS over the cross-level cell graph: 4-way moves between same-level in-mask
// cells, plus per-ramp edges between lower-entry and upper-exit cells.
fn bfs_reachability(masks: &[Mask], ramps: &[RampSpec], grid_cols: i32, grid_rows: i32) -> HashSet<(u32, i32, i32)> {
    let mut visited: HashSet<(u32, i32, i32)> = HashSet::new();
    let mut queue: VecDeque<(u32, i32, i32)> = VecDeque::new();

    let Some((r0, c0)) = first_in_mask(&masks[0]) else {
        return visited;
    };
    visited.insert((0, r0, c0));
    queue.push_back((0, r0, c0));

    while let Some((level, r, c)) = queue.pop_front() {
        // Same-level horizontal moves.
        for (dr, dc) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let nr = r + dr;
            let nc = c + dc;
            if nr < 0 || nr >= grid_rows || nc < 0 || nc >= grid_cols {
                continue;
            }
            if !masks[level as usize][nr as usize][nc as usize] {
                continue;
            }
            if visited.insert((level, nr, nc)) {
                queue.push_back((level, nr, nc));
            }
        }

        // Ramp endpoints: each ramp connects one lower_entry cell to one
        // upper_exit cell, traversable in both directions.
        for ramp in ramps {
            let lower = ramp.lower_level;
            let upper = lower + 1;

            if level == lower {
                if ramp.lower_entry_cells().contains(&(r, c)) {
                    for &(er, ec) in &ramp.upper_exit_cells() {
                        if !cell_in_mask(masks, upper, er, ec, grid_cols, grid_rows) {
                            continue;
                        }
                        if visited.insert((upper, er, ec)) {
                            queue.push_back((upper, er, ec));
                        }
                    }
                }
            } else if level == upper && ramp.upper_exit_cells().contains(&(r, c)) {
                for &(er, ec) in &ramp.lower_entry_cells() {
                    if !cell_in_mask(masks, lower, er, ec, grid_cols, grid_rows) {
                        continue;
                    }
                    if visited.insert((lower, er, ec)) {
                        queue.push_back((lower, er, ec));
                    }
                }
            }
        }
    }

    visited
}

fn cell_in_mask(masks: &[Mask], level: u32, r: i32, c: i32, grid_cols: i32, grid_rows: i32) -> bool {
    if r < 0 || r >= grid_rows || c < 0 || c >= grid_cols {
        return false;
    }
    let level_idx = level as usize;
    if level_idx >= masks.len() {
        return false;
    }
    masks[level_idx][r as usize][c as usize]
}

fn first_in_mask(mask: &Mask) -> Option<(i32, i32)> {
    for (r, row) in mask.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            if v {
                return Some((r as i32, c as i32));
            }
        }
    }
    None
}

// Clear cells from upper-level masks that sit directly above an accepted ramp's
// footprint. The vertical space above the ramp is reserved for the ramp's
// triangular volume; a floor slab there would either z-fight with the ramp
// top or look like a low overhead covering the climb.
pub fn clear_footprint_cells_above_ramps(masks: &mut [Mask], ramps: &[RampSpec]) {
    for ramp in ramps {
        let upper_level = (ramp.lower_level + 1) as usize;
        if upper_level >= masks.len() {
            continue;
        }
        for (row, col) in ramp.footprint_cells() {
            masks[upper_level][row as usize][col as usize] = false;
        }
    }
}

// Drop unreachable cells from each mask (in-place). Returns the number of cells
// dropped per level.
pub fn prune_unreachable(masks: &mut [Mask], ramps: &[RampSpec], grid_cols: i32, grid_rows: i32) -> Vec<usize> {
    let reach = bfs_reachability(masks, ramps, grid_cols, grid_rows);
    let mut dropped = vec![0usize; masks.len()];
    for (level_idx, mask) in masks.iter_mut().enumerate() {
        for (r, row) in mask.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                if *cell && !reach.contains(&(level_idx as u32, r as i32, c as i32)) {
                    *cell = false;
                    dropped[level_idx] += 1;
                }
            }
        }
    }
    dropped
}

// Apply ramp flags to the level-0 grid only — the only grid we keep around.
// Ramps at higher levels don't need their cells flagged because no Phase 2
// helper reads them yet.
pub fn apply_to_level0_grid(grid: &mut [Vec<GridCell>], ramps: &[RampSpec]) {
    for ramp in ramps {
        if ramp.lower_level != 0 {
            continue;
        }
        for (row, col) in ramp.footprint_cells() {
            grid[row as usize][col as usize].has_ramp = true;
        }
        if ramp.along_x {
            for row in ramp.row0..ramp.row_end {
                if ramp.high_at_end {
                    grid[row as usize][ramp.col0 as usize].ramp_base_west = true;
                    grid[row as usize][(ramp.col_end - 1) as usize].ramp_top_east = true;
                } else {
                    grid[row as usize][(ramp.col_end - 1) as usize].ramp_base_east = true;
                    grid[row as usize][ramp.col0 as usize].ramp_top_west = true;
                }
            }
        } else {
            for col in ramp.col0..ramp.col_end {
                if ramp.high_at_end {
                    grid[ramp.row0 as usize][col as usize].ramp_base_north = true;
                    grid[(ramp.row_end - 1) as usize][col as usize].ramp_top_south = true;
                } else {
                    grid[(ramp.row_end - 1) as usize][col as usize].ramp_base_south = true;
                    grid[ramp.row0 as usize][col as usize].ramp_top_north = true;
                }
            }
        }
    }
}

pub fn specs_to_ramps(specs: &[RampSpec]) -> Vec<Ramp> {
    specs.iter().map(RampSpec::to_ramp).collect()
}
