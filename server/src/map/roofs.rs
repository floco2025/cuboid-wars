use rand::Rng;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::{
    constants::{OVERLAP_ROOFS, ROOF_NEIGHBOR_PREFERENCE, ROOF_NUM_SEGMENTS},
    resources::GridCell,
};
use super::helpers::count_cell_walls;
use common::{
    constants::*,
    protocol::Roof,
};

const MERGE_EPS: f32 = 0.01;
const CORNER_EPS: f32 = 0.01; // Small inset to avoid overlap for edge fillers

// Generate individual roof segments (no merging) covering full grid cells.
// Returns roofs and updated grid with has_roof flags set.
#[must_use]
pub fn generate_individual_roofs(
    mut grid: Vec<Vec<GridCell>>,
    grid_cols: i32,
    grid_rows: i32,
) -> (Vec<Roof>, Vec<Vec<GridCell>>) {
    let mut rng = rand::rng();

    // Count walls for each cell
    let mut wall_counts = vec![vec![0u8; grid_cols as usize]; grid_rows as usize];

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            wall_counts[row as usize][col as usize] = count_cell_walls(grid[row as usize][col as usize]);
        }
    }

    let mut roof_cells: HashSet<(i32, i32)> = HashSet::new();

    // Phase 1: Find all cells adjacent to ramp tops
    let mut ramp_top_adjacent: Vec<(i32, i32)> = Vec::new();

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let cell = grid[row as usize][col as usize];

            // Check each elevated edge and collect adjacent cells
            if cell.ramp_top_north && row > 0 {
                let neighbor = (row - 1, col);
                if !grid[neighbor.0 as usize][neighbor.1 as usize].has_ramp {
                    ramp_top_adjacent.push(neighbor);
                }
            }
            if cell.ramp_top_south && row < grid_rows - 1 {
                let neighbor = (row + 1, col);
                if !grid[neighbor.0 as usize][neighbor.1 as usize].has_ramp {
                    ramp_top_adjacent.push(neighbor);
                }
            }
            if cell.ramp_top_west && col > 0 {
                let neighbor = (row, col - 1);
                if !grid[neighbor.0 as usize][neighbor.1 as usize].has_ramp {
                    ramp_top_adjacent.push(neighbor);
                }
            }
            if cell.ramp_top_east && col < grid_cols - 1 {
                let neighbor = (row, col + 1);
                if !grid[neighbor.0 as usize][neighbor.1 as usize].has_ramp {
                    ramp_top_adjacent.push(neighbor);
                }
            }
        }
    }

    // Connect each ramp-top-adjacent cell to its nearest neighbor
    let mut connected_pairs: HashSet<(i32, i32)> = HashSet::new();

    for (i, &start) in ramp_top_adjacent.iter().enumerate() {
        if connected_pairs.contains(&start) {
            continue; // This one already has at least one connection
        }

        // Find nearest other ramp-adjacent cell
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
            // BFS to find shortest path from start to target
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

                // Check all 4 neighbors
                let neighbors = [
                    (row - 1, col), // North
                    (row + 1, col), // South
                    (row, col - 1), // West
                    (row, col + 1), // East
                ];

                for &(nr, nc) in &neighbors {
                    if nr < 0 || nr >= grid_rows || nc < 0 || nc >= grid_cols {
                        continue;
                    }
                    if visited.contains(&(nr, nc)) || grid[nr as usize][nc as usize].has_ramp {
                        continue;
                    }

                    visited.insert((nr, nc));
                    parent.insert((nr, nc), (row, col));
                    queue.push_back((nr, nc));
                }
            }

            // Trace back path and add to roof_cells
            if found {
                let mut current = target;
                while current != start {
                    roof_cells.insert(current);
                    connected_pairs.insert(current);
                    if let Some(&prev) = parent.get(&current) {
                        current = prev;
                    } else {
                        break;
                    }
                }
                roof_cells.insert(start);
                connected_pairs.insert(start);
            }
        }
    }

    // Phase 2: Place remaining roofs using weighted selection
    // Iteratively place roofs until we reach target count
    while roof_cells.len() < ROOF_NUM_SEGMENTS {
        // Build weighted list of candidate cells
        let mut candidates = Vec::new();

        for row in 0..grid_rows {
            for col in 0..grid_cols {
                if roof_cells.contains(&(row, col)) || grid[row as usize][col as usize].has_ramp {
                    continue;
                }

                let wall_count = wall_counts[row as usize][col as usize];
                let cell = grid[row as usize][col as usize];

                // Count roofed neighbors that don't have walls between them
                let mut neighbor_count = 0;

                // North neighbor (row - 1)
                if row > 0 && !cell.has_north_wall && roof_cells.contains(&(row - 1, col)) {
                    neighbor_count += 1;
                }
                // South neighbor (row + 1)
                if row < grid_rows - 1 && !cell.has_south_wall && roof_cells.contains(&(row + 1, col)) {
                    neighbor_count += 1;
                }
                // West neighbor (col - 1)
                if col > 0 && !cell.has_west_wall && roof_cells.contains(&(row, col - 1)) {
                    neighbor_count += 1;
                }
                // East neighbor (col + 1)
                if col < grid_cols - 1 && !cell.has_east_wall && roof_cells.contains(&(row, col + 1)) {
                    neighbor_count += 1;
                }

                // Skip cells with <2 walls unless they have at least two roofed neighbors
                if wall_count < 2 && neighbor_count < 2 {
                    continue;
                }

                // Weight = base weight * neighbor multiplier
                let base_weight = if wall_count >= 2 { 1.0 } else { 0.5 };
                let neighbor_multiplier = 1.0 + (f64::from(neighbor_count) * ROOF_NEIGHBOR_PREFERENCE);
                let weight = base_weight * neighbor_multiplier;

                candidates.push(((row, col), weight));
            }
        }

        if candidates.is_empty() {
            break; // No valid candidates left
        }

        // Pick weighted random candidate
        let total_weight: f64 = candidates.iter().map(|(_, w)| w).sum();
        let mut pick = rng.random_range(0.0..total_weight);

        for ((row, col), weight) in candidates {
            pick -= weight;
            if pick <= 0.0 {
                roof_cells.insert((row, col));
                break;
            }
        }
    }

    // Convert roof cells to individual Roof segments
    let mut roofs = Vec::new();

    for &(row, col) in &roof_cells {
        // Calculate world coordinates
        let (world_x1, world_x2, world_z1, world_z2, edge_fillers) = if OVERLAP_ROOFS {
            // Overlap mode: extend on all sides by roof_thickness/2 for guaranteed coverage
            let x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)) - WALL_THICKNESS / 2.0;
            let x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)) + WALL_THICKNESS / 2.0;
            let z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)) - WALL_THICKNESS / 2.0;
            let z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)) + WALL_THICKNESS / 2.0;
            (x1, x2, z1, z2, Vec::new())
        } else {
            // Non-overlap mode: extend outward unless a neighboring roof would overlap
            let mut x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let mut x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let mut z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            let mut z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            let mut edge_fillers: Vec<Roof> = Vec::new();

            // Neighbor checks for overlap control
            let neighbor_w = col > 0 && roof_cells.contains(&(row, col - 1));
            let neighbor_e = col < grid_cols - 1 && roof_cells.contains(&(row, col + 1));
            let neighbor_n = row > 0 && roof_cells.contains(&(row - 1, col));
            let neighbor_s = row < grid_rows - 1 && roof_cells.contains(&(row + 1, col));

            let neighbor_nw = row > 0 && col > 0 && roof_cells.contains(&(row - 1, col - 1));
            let neighbor_ne = row > 0 && col < grid_cols - 1 && roof_cells.contains(&(row - 1, col + 1));
            let neighbor_sw = row < grid_rows - 1 && col > 0 && roof_cells.contains(&(row + 1, col - 1));
            let neighbor_se = row < grid_rows - 1 && col < grid_cols - 1 && roof_cells.contains(&(row + 1, col + 1));

            // Decide which sides can extend; suppress diagonal corner overlaps
            let extend_w = !neighbor_w;
            let extend_e = !neighbor_e;
            let mut extend_n = !neighbor_n;
            let mut extend_s = !neighbor_s;

            // For diagonal neighbors, trim the axis pointing toward them (vertical for north/south diagonals)
            // to prevent corner overlap while keeping lateral length where possible.
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

            // Edge fillers: if a diagonal blocks north/south but no direct neighbor, add a thin strip inset from corners
            let north_ramp = row > 0 && grid[(row - 1) as usize][col as usize].has_ramp;
            let south_ramp = row < grid_rows - 1 && grid[(row + 1) as usize][col as usize].has_ramp;
            let pad = (WALL_THICKNESS / 2.0) - CORNER_EPS;
            if pad > 0.0 {
                if !extend_n && !neighbor_n && !north_ramp && (neighbor_nw || neighbor_ne) {
                    let fx1 = if neighbor_nw { x1 + pad } else { x1 };
                    let fx2 = if neighbor_ne { x2 - pad } else { x2 };
                    if fx2 > fx1 {
                        edge_fillers.push(Roof {
                            x1: fx1,
                            z1: z1 - pad,
                            x2: fx2,
                            z2: z1,
                            thickness: ROOF_THICKNESS,
                        });
                    }
                }
                if !extend_s && !neighbor_s && !south_ramp && (neighbor_sw || neighbor_se) {
                    let fx1 = if neighbor_sw { x1 + pad } else { x1 };
                    let fx2 = if neighbor_se { x2 - pad } else { x2 };
                    if fx2 > fx1 {
                        edge_fillers.push(Roof {
                            x1: fx1,
                            z1: z2,
                            x2: fx2,
                            z2: z2 + pad,
                            thickness: ROOF_THICKNESS,
                        });
                    }
                }
            }

            (x1, x2, z1, z2, edge_fillers)
        };

        roofs.push(Roof {
            x1: world_x1,
            z1: world_z1,
            x2: world_x2,
            z2: world_z2,
            thickness: ROOF_THICKNESS,
        });

        roofs.extend(edge_fillers);

        // Mark cell as having a roof
        grid[row as usize][col as usize].has_roof = true;
    }

    (roofs, grid)
}

// Merge adjacent roofs into larger segments
pub fn merge_roofs(mut roofs: Vec<Roof>) -> Vec<Roof> {
    // Normalize ordering
    for r in &mut roofs {
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
        let mut used = vec![false; roofs.len()];
        let mut out: Vec<Roof> = Vec::new();

        for i in 0..roofs.len() {
            if used[i] {
                continue;
            }
            let mut acc = roofs[i];
            used[i] = true;

            let mut merged_this_round = true;
            while merged_this_round {
                merged_this_round = false;
                for j in 0..roofs.len() {
                    if used[j] {
                        continue;
                    }
                    let b = roofs[j];
                    let same_thickness = (acc.thickness - b.thickness).abs() < MERGE_EPS;
                    if !same_thickness {
                        continue;
                    }

                    // Horizontal merge: same z span, adjacent in x
                    let same_z_span = (acc.z1 - b.z1).abs() < MERGE_EPS && (acc.z2 - b.z2).abs() < MERGE_EPS;
                    let adjacent_x = (acc.x2 - b.x1).abs() < MERGE_EPS || (b.x2 - acc.x1).abs() < MERGE_EPS;

                    // Vertical merge: same x span, adjacent in z
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

        roofs = out;
    }

    roofs
}
