use rand::{Rng, rngs::ThreadRng};
use std::collections::{HashSet, VecDeque};

use crate::{
    constants::{
        MERGE_ROOF_SEGMENTS, MERGE_WALL_SEGMENTS, OVERLAP_ROOFS, ROOF_PROBABILITY_2_WALLS,
        ROOF_PROBABILITY_3_WALLS, ROOF_PROBABILITY_WITH_NEIGHBOR, WALL_2ND_PROBABILITY_RATIO,
        WALL_3RD_PROBABILITY_RATIO, WALL_NUM_SEGMENTS, OVERLAP_WALLS,
    },
    resources::{GridCell, GridConfig},
};
use common::{
    constants::*,
    protocol::{Position, Roof, Wall},
};

// ============================================================================
// Grid Helper Functions
// ============================================================================

#[must_use]
pub fn cell_center(grid_x: i32, grid_z: i32) -> Position {
    Position {
        x: (grid_x as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)),
        y: 0.0,
        z: (grid_z as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)),
    }
}

#[must_use]
pub fn grid_coords_from_position(pos: &Position) -> (i32, i32) {
    let grid_x = ((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32;
    let grid_z = ((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32;
    (grid_x, grid_z)
}

#[allow(clippy::implicit_hasher)]
pub fn find_unoccupied_cell(rng: &mut ThreadRng, occupied_cells: &HashSet<(i32, i32)>) -> Option<(i32, i32)> {
    const MAX_ATTEMPTS: usize = 100;
    for _ in 0..MAX_ATTEMPTS {
        let grid_x = rng.random_range(0..GRID_COLS);
        let grid_z = rng.random_range(0..GRID_ROWS);
        if !occupied_cells.contains(&(grid_x, grid_z)) {
            return Some((grid_x, grid_z));
        }
    }
    None
}

// Count how many walls a cell has (0-4)
const fn count_cell_walls(cell: GridCell) -> u8 {
    let mut count = 0;
    if cell.has_north_wall {
        count += 1;
    }
    if cell.has_south_wall {
        count += 1;
    }
    if cell.has_west_wall {
        count += 1;
    }
    if cell.has_east_wall {
        count += 1;
    }
    count
}

// Check if all grid cells are reachable using BFS
fn all_cells_reachable(grid: &[Vec<GridCell>], grid_cols: i32, grid_rows: i32) -> bool {
    if grid_cols <= 0 || grid_rows <= 0 {
        return true;
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let target_count = (grid_rows * grid_cols) as usize;

    // Start from cell (0, 0)
    queue.push_back((0, 0));
    visited.insert((0, 0));

    while let Some((row, col)) = queue.pop_front() {
        let cell = &grid[row as usize][col as usize];

        // Check all 4 directions
        // North
        if row > 0 && !cell.has_north_wall && !visited.contains(&(row - 1, col)) {
            visited.insert((row - 1, col));
            queue.push_back((row - 1, col));
        }

        // South
        if row < grid_rows - 1 && !cell.has_south_wall && !visited.contains(&(row + 1, col)) {
            visited.insert((row + 1, col));
            queue.push_back((row + 1, col));
        }

        // West
        if col > 0 && !cell.has_west_wall && !visited.contains(&(row, col - 1)) {
            visited.insert((row, col - 1));
            queue.push_back((row, col - 1));
        }

        // East
        if col < grid_cols - 1 && !cell.has_east_wall && !visited.contains(&(row, col + 1)) {
            visited.insert((row, col + 1));
            queue.push_back((row, col + 1));
        }

        if visited.len() == target_count {
            return true;
        }
    }

    // All cells should be reachable
    visited.len() == (grid_rows * grid_cols) as usize
}

// ============================================================================
// Grid Generation
// ============================================================================

// Walls are placed along grid lines in a maze-like pattern.
// Ensures all grid cells remain reachable from each other.
// Always places walls around the perimeter of the field.
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

        // Count existing walls in both cells adjacent to this potential wall
        let cell1_walls = count_cell_walls(*cell);
        let cell2_walls = match direction {
            0 => {
                // South wall - check cell below
                if row < grid_rows - 1 {
                    count_cell_walls(grid[(row + 1) as usize][col as usize])
                } else {
                    0
                }
            }
            1 => {
                // East wall - check cell to the right
                if col < grid_cols - 1 {
                    count_cell_walls(grid[row as usize][(col + 1) as usize])
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
        if all_cells_reachable(&grid, grid_cols, grid_rows) {
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
    let mut walls = generate_individual_walls(&grid, grid_cols, grid_rows);
    if MERGE_WALL_SEGMENTS && !OVERLAP_WALLS {
        walls = merge_walls(walls);
    }

    // Generate roofs based on grid
    let mut roofs = generate_individual_roofs(&grid, grid_cols, grid_rows);
    if MERGE_ROOF_SEGMENTS && !OVERLAP_ROOFS {
        roofs = merge_roofs(roofs);
    }

    GridConfig { grid, walls, roofs }
}

// Does the grid line at (row, col) have a horizontal wall? Handles perimeter rows.
#[inline]
fn has_horizontal_wall(grid: &[Vec<GridCell>], row: i32, col: i32, grid_rows: i32) -> bool {
    if row == 0 {
        grid[0][col as usize].has_north_wall
    } else if row == grid_rows {
        grid[(grid_rows - 1) as usize][col as usize].has_south_wall
    } else {
        grid[row as usize][col as usize].has_north_wall
    }
}

// Does the grid line at (row, col) have a vertical wall? Handles perimeter columns.
#[inline]
fn has_vertical_wall(grid: &[Vec<GridCell>], row: i32, col: i32, grid_cols: i32) -> bool {
    if col == 0 {
        grid[row as usize][0].has_west_wall
    } else if col == grid_cols {
        grid[row as usize][(grid_cols - 1) as usize].has_east_wall
    } else {
        grid[row as usize][col as usize].has_west_wall
    }
}

// For a vertical wall at (row, col), report if horizontal walls meet its top/bottom ends (edge-safe)
#[inline]
fn perpendicular_horizontal_walls(
    grid: &[Vec<GridCell>],
    row: i32,
    col: i32,
    grid_cols: i32,
    grid_rows: i32,
) -> (bool, bool) {
    // Top endpoint is at grid line `row`; check horizontal walls on both sides of the vertical line.
    let has_perp_top = row > 0
        && (
            (col < grid_cols && has_horizontal_wall(grid, row, col, grid_rows)) // right side (guarded)
                || (col > 0 && has_horizontal_wall(grid, row, col - 1, grid_rows)) // left side (guarded)
        );

    // Bottom endpoint is at grid line `row + 1`; check the horizontals that meet there.
    let has_perp_bottom = row < grid_rows
        && (
            (col < grid_cols && has_horizontal_wall(grid, row + 1, col, grid_rows)) // right side (guarded)
                || (col > 0 && has_horizontal_wall(grid, row + 1, col - 1, grid_rows)) // left side (guarded)
        );

    (has_perp_top, has_perp_bottom)
}

// Generate individual wall segments (no merging) with gap-filling extensions
#[must_use]
fn generate_individual_walls(grid: &[Vec<GridCell>], grid_cols: i32, grid_rows: i32) -> Vec<Wall> {
    let mut walls = Vec::new();

    // Process horizontal walls (north/south edges)
    for row in 0..=grid_rows {
        for col in 0..grid_cols {
            if !has_horizontal_wall(grid, row, col, grid_rows) {
                continue;
            }

            // Check for adjacent horizontal walls
            let has_left = col > 0 && has_horizontal_wall(grid, row, col - 1, grid_rows);
            let has_right = col < grid_cols - 1 && has_horizontal_wall(grid, row, col + 1, grid_rows);

            // Detect vertical walls passing through the left and right endpoints (true T vs corner)
            let left_vert_top = row > 0 && has_vertical_wall(grid, row - 1, col, grid_cols);
            let left_vert_bottom = row < grid_rows && has_vertical_wall(grid, row, col, grid_cols);
            let left_vert_through = left_vert_top && left_vert_bottom;

            let right_vert_top = row > 0 && has_vertical_wall(grid, row - 1, col + 1, grid_cols);
            let right_vert_bottom = row < grid_rows && has_vertical_wall(grid, row, col + 1, grid_cols);
            let right_vert_through = right_vert_top && right_vert_bottom;

            let world_z = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            // Horizontal walls inset only when a vertical passes through (T); extend otherwise for corners/ends
            let x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0))
                + if OVERLAP_WALLS {
                    -WALL_WIDTH / 2.0
                } else if left_vert_through && !has_left {
                    WALL_WIDTH / 2.0 // inset at T so vertical can pass through
                } else if !has_left {
                    -WALL_WIDTH / 2.0 // extend to meet corners or isolated ends
                } else {
                    0.0
                };
            let x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0))
                + if OVERLAP_WALLS {
                    WALL_WIDTH / 2.0
                } else if right_vert_through && !has_right {
                    -WALL_WIDTH / 2.0 // inset at T on the right
                } else if !has_right {
                    WALL_WIDTH / 2.0 // extend for corners or isolated ends
                } else {
                    0.0
                };

            walls.push(Wall {
                x1,
                z1: world_z,
                x2,
                z2: world_z,
                width: WALL_WIDTH,
            });
        }
    }

    // Process vertical walls (west/east edges)
    for col in 0..=grid_cols {
        for row in 0..grid_rows {
            if !has_vertical_wall(grid, row, col, grid_cols) {
                continue;
            }

            // Check for adjacent vertical walls
            let has_top = row > 0 && has_vertical_wall(grid, row - 1, col, grid_cols);
            let has_bottom = row < grid_rows - 1 && has_vertical_wall(grid, row + 1, col, grid_cols);

            // Check for perpendicular horizontal walls at ends (for L-corners)
            let (has_perp_top, has_perp_bottom) = perpendicular_horizontal_walls(grid, row, col, grid_cols, grid_rows);

            let world_x = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0))
                + if has_perp_top && !has_top {
                    WALL_WIDTH / 2.0 // Inset for L-corner
                } else if !has_top && !has_perp_top {
                    -WALL_WIDTH / 2.0 // Extend for isolated end
                } else {
                    0.0
                };
            let z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0))
                + if has_perp_bottom && !has_bottom {
                    -WALL_WIDTH / 2.0 // Inset for L-corner
                } else if !has_bottom && !has_perp_bottom {
                    WALL_WIDTH / 2.0 // Extend for isolated end
                } else {
                    0.0
                };

            walls.push(Wall {
                x1: world_x,
                z1,
                x2: world_x,
                z2,
                width: WALL_WIDTH,
            });
        }
    }

    walls
}

// Generate individual roof segments (no merging) covering full grid cells
#[must_use]
fn generate_individual_roofs(grid: &[Vec<GridCell>], grid_cols: i32, grid_rows: i32) -> Vec<Roof> {
    let mut rng = rand::rng();

    // Count walls for each cell
    let mut wall_counts = vec![vec![0u8; grid_cols as usize]; grid_rows as usize];

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            wall_counts[row as usize][col as usize] = count_cell_walls(grid[row as usize][col as usize]);
        }
    }

    // Pass 1: Place roofs based on wall count
    let mut roof_cells: HashSet<(i32, i32)> = HashSet::new();

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let wall_count = wall_counts[row as usize][col as usize];

            let should_place_roof = match wall_count {
                2 => rng.random_bool(ROOF_PROBABILITY_2_WALLS),
                3 => rng.random_bool(ROOF_PROBABILITY_3_WALLS),
                _ => false,
            };

            if should_place_roof {
                roof_cells.insert((row, col));
            }
        }
    }

    // Pass 2: Cells with 2+ walls adjacent to a roof get ROOF_PROBABILITY_WITH_NEIGHBOR chance
    let mut added_more = true;
    while added_more {
        added_more = false;

        for row in 0..grid_rows {
            for col in 0..grid_cols {
                if roof_cells.contains(&(row, col)) || wall_counts[row as usize][col as usize] < 2 {
                    continue;
                }

                let neighbors = [(row - 1, col), (row + 1, col), (row, col - 1), (row, col + 1)];

                let has_neighbor_with_roof = neighbors
                    .iter()
                    .any(|&(r, c)| r >= 0 && r < grid_rows && c >= 0 && c < grid_cols && roof_cells.contains(&(r, c)));

                if has_neighbor_with_roof && rng.random_bool(ROOF_PROBABILITY_WITH_NEIGHBOR) {
                    roof_cells.insert((row, col));
                    added_more = true;
                }
            }
        }
    }

    // Convert roof cells to individual Roof segments
    let mut roofs = Vec::new();

    for &(row, col) in &roof_cells {
        // Calculate world coordinates
        let (world_x1, world_x2, world_z1, world_z2) = if OVERLAP_ROOFS {
            // Overlap mode: extend on all sides by roof_thickness/2 for guaranteed coverage
            let x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)) - WALL_WIDTH / 2.0;
            let x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)) + WALL_WIDTH / 2.0;
            let z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)) - WALL_WIDTH / 2.0;
            let z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)) + WALL_WIDTH / 2.0;
            (x1, x2, z1, z2)
        } else {
            // Non-overlap mode: extend outward unless a neighboring roof would overlap
            let mut x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let mut x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let mut z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            let mut z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

            // West: extend if no west neighbor roof
            let neighbor_w = col > 0 && roof_cells.contains(&(row, col - 1));
            if !neighbor_w {
                x1 -= WALL_WIDTH / 2.0;
            }

            // East: extend if no east neighbor roof
            let neighbor_e = col < grid_cols - 1 && roof_cells.contains(&(row, col + 1));
            if !neighbor_e {
                x2 += WALL_WIDTH / 2.0;
            }

            // North: extend if no north neighbor roof
            let neighbor_n = row > 0 && roof_cells.contains(&(row - 1, col));
            if !neighbor_n {
                z1 -= WALL_WIDTH / 2.0;
            }

            // South: extend if no south neighbor roof
            let neighbor_s = row < grid_rows - 1 && roof_cells.contains(&(row + 1, col));
            if !neighbor_s {
                z2 += WALL_WIDTH / 2.0;
            }

            (x1, x2, z1, z2)
        };

        roofs.push(Roof {
            x1: world_x1,
            z1: world_z1,
            x2: world_x2,
            z2: world_z2,
            thickness: WALL_WIDTH,
        });
    }

    roofs
}

// ============================================================================
// Merging helpers
// ============================================================================

const MERGE_EPS: f32 = 1e-4;

fn normalize_wall(mut w: Wall) -> Wall {
    if (w.z1 - w.z2).abs() < MERGE_EPS {
        // horizontal: order by x
        if w.x1 > w.x2 {
            std::mem::swap(&mut w.x1, &mut w.x2);
        }
    } else if (w.x1 - w.x2).abs() < MERGE_EPS {
        // vertical: order by z
        if w.z1 > w.z2 {
            std::mem::swap(&mut w.z1, &mut w.z2);
        }
    }
    w
}

fn merge_walls(walls: Vec<Wall>) -> Vec<Wall> {
    let mut horizontals = Vec::new();
    let mut verticals = Vec::new();
    let mut others = Vec::new();

    for w in walls {
        let w = normalize_wall(w);
        if (w.z1 - w.z2).abs() < MERGE_EPS {
            horizontals.push(w);
        } else if (w.x1 - w.x2).abs() < MERGE_EPS {
            verticals.push(w);
        } else {
            others.push(w);
        }
    }

    horizontals.sort_by(|a, b| {
        a.z1
            .partial_cmp(&b.z1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x1.partial_cmp(&b.x1).unwrap_or(std::cmp::Ordering::Equal))
    });
    verticals.sort_by(|a, b| {
        a.x1
            .partial_cmp(&b.x1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.z1.partial_cmp(&b.z1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut merged = Vec::new();

    let merge_line = |list: Vec<Wall>, is_horizontal: bool, out: &mut Vec<Wall>| {
        let mut iter = list.into_iter();
        if let Some(mut cur) = iter.next() {
            for w in iter {
                if is_horizontal {
                    if (cur.z1 - w.z1).abs() < MERGE_EPS && (cur.width - w.width).abs() < MERGE_EPS {
                        if w.x1 <= cur.x2 + MERGE_EPS {
                            cur.x2 = cur.x2.max(w.x2);
                            continue;
                        }
                    }
                } else if (cur.x1 - w.x1).abs() < MERGE_EPS && (cur.width - w.width).abs() < MERGE_EPS {
                    if w.z1 <= cur.z2 + MERGE_EPS {
                        cur.z2 = cur.z2.max(w.z2);
                        continue;
                    }
                }
                out.push(cur);
                cur = w;
            }
            out.push(cur);
        }
    };

    merge_line(horizontals, true, &mut merged);
    merge_line(verticals, false, &mut merged);
    merged.extend(others);
    merged
}

fn merge_roofs(mut roofs: Vec<Roof>) -> Vec<Roof> {
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
