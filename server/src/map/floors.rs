use rand::{RngExt, rng};
use std::collections::{HashMap, HashSet, VecDeque};

use super::helpers::count_cell_walls;
use super::ramps::Mask;
use crate::{
    constants::{FLOOR_NEIGHBOR_PREFERENCE, FLOOR_OVERLAP},
    resources::GridCell,
};
use common::{constants::*, protocol::Floor};

const MERGE_EPS: f32 = 0.01;
const CORNER_EPS: f32 = 0.01;

// Generate a tier of floor cells at `level` (Y = `y`) constrained to `prev_mask`.
//
// Phase 1 walks BFS paths between every pair of ramp-top-adjacent cells (so each
// ramp lands on a connected stretch of floor). Phase 2 grows the tier outward
// using a wall-aware weighted picker until `target_count` cells are reached or
// no candidates remain. The grid is mutated in-place to record `has_floor_above`
// on cells whose level the new tier sits on top of (only meaningful for level 1).
//
// Returns `(floors, mask_at_level)` — the per-cell floor segments (with
// extension/corner-filler logic identical to the original 2-level path) and a
// boolean mask of cells that ended up with floor at this level.
#[must_use]
pub fn generate_floor_tier(
    grid: &mut [Vec<GridCell>],
    prev_mask: &Mask,
    grid_cols: i32,
    grid_rows: i32,
    level: u8,
    y: f32,
    target_count: usize,
) -> (Vec<Floor>, Mask) {
    let mut rng = rng();

    let mut wall_counts = vec![vec![0u8; grid_cols as usize]; grid_rows as usize];
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            wall_counts[row as usize][col as usize] = count_cell_walls(grid[row as usize][col as usize]);
        }
    }

    let mut floor_cells: HashSet<(i32, i32)> = HashSet::new();

    // Phase 1: connect every ramp-top-adjacent cell to its nearest sibling via BFS.
    let mut ramp_top_adjacent: Vec<(i32, i32)> = Vec::new();
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let cell = grid[row as usize][col as usize];

            let candidates = [
                (cell.ramp_top_north, row - 1, col),
                (cell.ramp_top_south, row + 1, col),
                (cell.ramp_top_west, row, col - 1),
                (cell.ramp_top_east, row, col + 1),
            ];
            for (active, nr, nc) in candidates {
                if !active || nr < 0 || nr >= grid_rows || nc < 0 || nc >= grid_cols {
                    continue;
                }
                if !mask_contains(prev_mask, nr, nc) {
                    continue;
                }
                if !grid[nr as usize][nc as usize].has_ramp {
                    ramp_top_adjacent.push((nr, nc));
                }
            }
        }
    }

    // Every ramp-top cell must end up with floor under it; otherwise a player
    // walking off the top of a ramp would step into empty space.
    for &cell in &ramp_top_adjacent {
        floor_cells.insert(cell);
    }

    let mut connected_pairs: HashSet<(i32, i32)> = HashSet::new();

    for (i, &start) in ramp_top_adjacent.iter().enumerate() {
        if connected_pairs.contains(&start) {
            continue;
        }

        let mut nearest_target: Option<(i32, i32)> = None;
        let mut nearest_dist = i32::MAX;

        for (j, &other) in ramp_top_adjacent.iter().enumerate() {
            if i == j {
                continue;
            }
            let dist = (start.0 - other.0).abs() + (start.1 - other.1).abs();
            if dist < nearest_dist {
                nearest_dist = dist;
                nearest_target = Some(other);
            }
        }

        if let Some(target) = nearest_target {
            let mut queue = VecDeque::new();
            let mut visited: HashSet<(i32, i32)> = HashSet::new();
            let mut parent: HashMap<(i32, i32), (i32, i32)> = HashMap::new();

            queue.push_back(start);
            visited.insert(start);

            let mut found = false;
            while let Some((row, col)) = queue.pop_front() {
                if (row, col) == target {
                    found = true;
                    break;
                }

                let neighbors = [(row - 1, col), (row + 1, col), (row, col - 1), (row, col + 1)];

                for &(nr, nc) in &neighbors {
                    if nr < 0 || nr >= grid_rows || nc < 0 || nc >= grid_cols {
                        continue;
                    }
                    if visited.contains(&(nr, nc)) || grid[nr as usize][nc as usize].has_ramp {
                        continue;
                    }
                    if !mask_contains(prev_mask, nr, nc) {
                        continue;
                    }

                    visited.insert((nr, nc));
                    parent.insert((nr, nc), (row, col));
                    queue.push_back((nr, nc));
                }
            }

            if found {
                let mut current = target;
                while current != start {
                    floor_cells.insert(current);
                    connected_pairs.insert(current);
                    if let Some(&prev) = parent.get(&current) {
                        current = prev;
                    } else {
                        break;
                    }
                }
                floor_cells.insert(start);
                connected_pairs.insert(start);
            }
        }
    }

    // Phase 2: grow tier outward via wall-aware weighted pick until we hit target.
    while floor_cells.len() < target_count {
        let mut candidates = Vec::new();

        for row in 0..grid_rows {
            for col in 0..grid_cols {
                if floor_cells.contains(&(row, col)) || grid[row as usize][col as usize].has_ramp {
                    continue;
                }
                if !mask_contains(prev_mask, row, col) {
                    continue;
                }

                let wall_count = wall_counts[row as usize][col as usize];
                let cell = grid[row as usize][col as usize];

                let mut neighbor_count = 0;
                if row > 0 && !cell.has_north_wall && floor_cells.contains(&(row - 1, col)) {
                    neighbor_count += 1;
                }
                if row < grid_rows - 1 && !cell.has_south_wall && floor_cells.contains(&(row + 1, col)) {
                    neighbor_count += 1;
                }
                if col > 0 && !cell.has_west_wall && floor_cells.contains(&(row, col - 1)) {
                    neighbor_count += 1;
                }
                if col < grid_cols - 1 && !cell.has_east_wall && floor_cells.contains(&(row, col + 1)) {
                    neighbor_count += 1;
                }

                if wall_count < 2 && neighbor_count < 2 {
                    continue;
                }

                let base_weight = if wall_count >= 2 { 1.0 } else { 0.5 };
                let neighbor_multiplier = 1.0 + (f64::from(neighbor_count) * FLOOR_NEIGHBOR_PREFERENCE);
                let weight = base_weight * neighbor_multiplier;

                candidates.push(((row, col), weight));
            }
        }

        if candidates.is_empty() {
            break;
        }

        let total_weight: f64 = candidates.iter().map(|(_, w)| w).sum();
        let mut pick = rng.random_range(0.0..total_weight);

        for ((row, col), weight) in candidates {
            pick -= weight;
            if pick <= 0.0 {
                floor_cells.insert((row, col));
                break;
            }
        }
    }

    // Build per-cell Floor segments with extend/corner-filler logic.
    let mut floors = Vec::new();
    let mut mask: Mask = vec![vec![false; grid_cols as usize]; grid_rows as usize];

    for &(row, col) in &floor_cells {
        let (world_x1, world_x2, world_z1, world_z2, edge_fillers) = if FLOOR_OVERLAP {
            let x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)) - WALL_THICKNESS / 2.0;
            let x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)) + WALL_THICKNESS / 2.0;
            let z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)) - WALL_THICKNESS / 2.0;
            let z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)) + WALL_THICKNESS / 2.0;
            (x1, x2, z1, z2, Vec::new())
        } else {
            let mut x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let mut x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let mut z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            let mut z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            let mut edge_fillers: Vec<Floor> = Vec::new();

            let neighbor_w = col > 0 && floor_cells.contains(&(row, col - 1));
            let neighbor_e = col < grid_cols - 1 && floor_cells.contains(&(row, col + 1));
            let neighbor_n = row > 0 && floor_cells.contains(&(row - 1, col));
            let neighbor_s = row < grid_rows - 1 && floor_cells.contains(&(row + 1, col));

            let neighbor_nw = row > 0 && col > 0 && floor_cells.contains(&(row - 1, col - 1));
            let neighbor_ne = row > 0 && col < grid_cols - 1 && floor_cells.contains(&(row - 1, col + 1));
            let neighbor_sw = row < grid_rows - 1 && col > 0 && floor_cells.contains(&(row + 1, col - 1));
            let neighbor_se = row < grid_rows - 1 && col < grid_cols - 1 && floor_cells.contains(&(row + 1, col + 1));

            let extend_w = !neighbor_w;
            let extend_e = !neighbor_e;
            let mut extend_n = !neighbor_n;
            let mut extend_s = !neighbor_s;

            if neighbor_nw || neighbor_ne {
                extend_n = false;
            }
            if neighbor_sw || neighbor_se {
                extend_s = false;
            }

            if extend_w {
                x1 -= WALL_THICKNESS / 2.0;
            }
            if extend_e {
                x2 += WALL_THICKNESS / 2.0;
            }
            if extend_n {
                z1 -= WALL_THICKNESS / 2.0;
            }
            if extend_s {
                z2 += WALL_THICKNESS / 2.0;
            }

            let north_ramp = row > 0 && grid[(row - 1) as usize][col as usize].has_ramp;
            let south_ramp = row < grid_rows - 1 && grid[(row + 1) as usize][col as usize].has_ramp;
            let pad = (WALL_THICKNESS / 2.0) - CORNER_EPS;
            if pad > 0.0 {
                if !extend_n && !neighbor_n && !north_ramp && (neighbor_nw || neighbor_ne) {
                    let fx1 = if neighbor_nw { x1 + pad } else { x1 };
                    let fx2 = if neighbor_ne { x2 - pad } else { x2 };
                    if fx2 > fx1 {
                        edge_fillers.push(Floor {
                            x1: fx1,
                            z1: z1 - pad,
                            x2: fx2,
                            z2: z1,
                            y,
                            thickness: FLOOR_THICKNESS,
                            level,
                        });
                    }
                }
                if !extend_s && !neighbor_s && !south_ramp && (neighbor_sw || neighbor_se) {
                    let fx1 = if neighbor_sw { x1 + pad } else { x1 };
                    let fx2 = if neighbor_se { x2 - pad } else { x2 };
                    if fx2 > fx1 {
                        edge_fillers.push(Floor {
                            x1: fx1,
                            z1: z2,
                            x2: fx2,
                            z2: z2 + pad,
                            y,
                            thickness: FLOOR_THICKNESS,
                            level,
                        });
                    }
                }
            }

            (x1, x2, z1, z2, edge_fillers)
        };

        floors.push(Floor {
            x1: world_x1,
            z1: world_z1,
            x2: world_x2,
            z2: world_z2,
            y,
            thickness: FLOOR_THICKNESS,
            level,
        });

        floors.extend(edge_fillers);

        mask[row as usize][col as usize] = true;
        if level == 1 {
            grid[row as usize][col as usize].has_floor_above = true;
        }
    }

    (floors, mask)
}

#[inline]
fn mask_contains(mask: &Mask, row: i32, col: i32) -> bool {
    if row < 0 || col < 0 {
        return false;
    }
    let r = row as usize;
    let c = col as usize;
    if r >= mask.len() || c >= mask[0].len() {
        return false;
    }
    mask[r][c]
}

// Merge adjacent floors at the same level into larger segments.
pub fn merge_floors(mut floors: Vec<Floor>) -> Vec<Floor> {
    for r in &mut floors {
        if r.x1 > r.x2 {
            std::mem::swap(&mut r.x1, &mut r.x2);
        }
        if r.z1 > r.z2 {
            std::mem::swap(&mut r.z1, &mut r.z2);
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        let mut used = vec![false; floors.len()];
        let mut out: Vec<Floor> = Vec::new();

        for i in 0..floors.len() {
            if used[i] {
                continue;
            }
            let mut acc = floors[i];
            used[i] = true;

            let mut merged_this_round = true;
            while merged_this_round {
                merged_this_round = false;
                for j in 0..floors.len() {
                    if used[j] {
                        continue;
                    }
                    let b = floors[j];
                    let same_thickness = (acc.thickness - b.thickness).abs() < MERGE_EPS;
                    let same_level = acc.level == b.level;
                    let same_y = (acc.y - b.y).abs() < MERGE_EPS;
                    if !same_thickness || !same_level || !same_y {
                        continue;
                    }

                    let same_z_span = (acc.z1 - b.z1).abs() < MERGE_EPS && (acc.z2 - b.z2).abs() < MERGE_EPS;
                    let adjacent_x = (acc.x2 - b.x1).abs() < MERGE_EPS || (b.x2 - acc.x1).abs() < MERGE_EPS;

                    let same_x_span = (acc.x1 - b.x1).abs() < MERGE_EPS && (acc.x2 - b.x2).abs() < MERGE_EPS;
                    let adjacent_z = (acc.z2 - b.z1).abs() < MERGE_EPS || (b.z2 - acc.z1).abs() < MERGE_EPS;

                    if (same_z_span && adjacent_x) || (same_x_span && adjacent_z) {
                        acc.x1 = acc.x1.min(b.x1);
                        acc.x2 = acc.x2.max(b.x2);
                        acc.z1 = acc.z1.min(b.z1);
                        acc.z2 = acc.z2.max(b.z2);
                        used[j] = true;
                        merged_this_round = true;
                        changed = true;
                    }
                }
            }
            out.push(acc);
        }

        floors = out;
    }

    floors
}
