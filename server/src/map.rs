use rand::{Rng, rngs::ThreadRng};
use std::collections::{HashSet, VecDeque};

use crate::{
    constants::{
        MERGE_ROOF_SEGMENTS, MERGE_WALL_SEGMENTS, OVERLAP_ROOFS, OVERLAP_WALLS, ROOF_NEIGHBOR_PREFERENCE,
        ROOF_NUM_SEGMENTS, RAMP_COUNT, RAMP_LENGTH_CELLS, RAMP_MIN_SEPARATION_CELLS, RAMP_WIDTH_CELLS,
        WALL_2ND_PROBABILITY_RATIO, WALL_3RD_PROBABILITY_RATIO, WALL_NUM_SEGMENTS,
    },
    resources::{GridCell, GridConfig},
};
use common::{
    constants::*,
    protocol::{Position, Ramp, Roof, Wall},
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
    
    // Count only non-ramp cells as the target
    let mut target_count = 0;
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            if !grid[row as usize][col as usize].has_ramp {
                target_count += 1;
            }
        }
    }

    // Start from first non-ramp cell
    let mut start_found = false;
    'find_start: for row in 0..grid_rows {
        for col in 0..grid_cols {
            if !grid[row as usize][col as usize].has_ramp {
                queue.push_back((row, col));
                visited.insert((row, col));
                start_found = true;
                break 'find_start;
            }
        }
    }
    
    if !start_found {
        return true; // No non-ramp cells to check
    }

    while let Some((row, col)) = queue.pop_front() {
        let cell = &grid[row as usize][col as usize];

        // Check all 4 directions - consider both walls and ramps
        // North
        if row > 0 && !cell.has_north_wall && !visited.contains(&(row - 1, col)) {
            let next_cell = &grid[(row - 1) as usize][col as usize];
            // Can only move to non-ramp cells
            if !next_cell.has_ramp {
                visited.insert((row - 1, col));
                queue.push_back((row - 1, col));
            }
        }

        // South
        if row < grid_rows - 1 && !cell.has_south_wall && !visited.contains(&(row + 1, col)) {
            let next_cell = &grid[(row + 1) as usize][col as usize];
            if !next_cell.has_ramp {
                visited.insert((row + 1, col));
                queue.push_back((row + 1, col));
            }
        }

        // West
        if col > 0 && !cell.has_west_wall && !visited.contains(&(row, col - 1)) {
            let next_cell = &grid[row as usize][(col - 1) as usize];
            if !next_cell.has_ramp {
                visited.insert((row, col - 1));
                queue.push_back((row, col - 1));
            }
        }

        // East
        if col < grid_cols - 1 && !cell.has_east_wall && !visited.contains(&(row, col + 1)) {
            let next_cell = &grid[row as usize][(col + 1) as usize];
            if !next_cell.has_ramp {
                visited.insert((row, col + 1));
                queue.push_back((row, col + 1));
            }
        }

        if visited.len() == target_count {
            return true;
        }
    }

    // All non-ramp cells should be reachable
    visited.len() == target_count
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

    // Generate ramps early so wall placement can respect ramp bases
    let ramps = generate_ramps(&mut grid, grid_cols, grid_rows);

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
            0 => cell.ramp_base_south
                || cell.ramp_top_south
                || (row + 1 < grid_rows
                    && (grid[(row + 1) as usize][col as usize].ramp_base_north
                        || grid[(row + 1) as usize][col as usize].ramp_top_north))
                || (cell.has_ramp
                    && row + 1 < grid_rows
                    && grid[(row + 1) as usize][col as usize].has_ramp),
            // east wall between (row,col) and (row,col+1)
            1 => cell.ramp_base_east
                || cell.ramp_top_east
                || (col + 1 < grid_cols
                    && (grid[row as usize][(col + 1) as usize].ramp_base_west
                        || grid[row as usize][(col + 1) as usize].ramp_top_west))
                || (cell.has_ramp
                    && col + 1 < grid_cols
                    && grid[row as usize][(col + 1) as usize].has_ramp),
            _ => false,
        };
        if ramp_blocked {
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

    // Separate walls into boundary and interior
    let half_field_width = FIELD_WIDTH / 2.0;
    let half_field_depth = FIELD_DEPTH / 2.0;
    let epsilon = 0.01;

    let (boundary_walls, interior_walls): (Vec<Wall>, Vec<Wall>) = walls.iter().partition(|w| {
        // Check if wall is at the boundary (within epsilon)
        let at_left = (w.x1 + half_field_width).abs() < epsilon && (w.x2 + half_field_width).abs() < epsilon;
        let at_right = (w.x1 - half_field_width).abs() < epsilon && (w.x2 - half_field_width).abs() < epsilon;
        let at_top = (w.z1 + half_field_depth).abs() < epsilon && (w.z2 + half_field_depth).abs() < epsilon;
        let at_bottom = (w.z1 - half_field_depth).abs() < epsilon && (w.z2 - half_field_depth).abs() < epsilon;
        at_left || at_right || at_top || at_bottom
    });

    GridConfig {
        grid,
        boundary_walls,
        interior_walls,
        all_walls: walls,
        roofs,
        ramps,
    }
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
                || (col > 0 && has_horizontal_wall(grid, row, col - 1, grid_rows))
            // left side (guarded)
        );

    // Bottom endpoint is at grid line `row + 1`; check the horizontals that meet there.
    let has_perp_bottom = row < grid_rows
        && (
            (col < grid_cols && has_horizontal_wall(grid, row + 1, col, grid_rows)) // right side (guarded)
                || (col > 0 && has_horizontal_wall(grid, row + 1, col - 1, grid_rows))
            // left side (guarded)
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
            let mut parent: std::collections::HashMap<(i32, i32), (i32, i32)> = std::collections::HashMap::new();

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
            thickness: ROOF_THICKNESS,
        });
    }

    roofs
}

// Generate ramps as right triangular prisms using opposite corners
fn generate_ramps(grid: &mut [Vec<GridCell>], grid_cols: i32, grid_rows: i32) -> Vec<Ramp> {
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
        a.z1.partial_cmp(&b.z1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x1.partial_cmp(&b.x1).unwrap_or(std::cmp::Ordering::Equal))
    });
    verticals.sort_by(|a, b| {
        a.x1.partial_cmp(&b.x1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.z1.partial_cmp(&b.z1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut merged = Vec::new();

    let merge_line = |list: Vec<Wall>, is_horizontal: bool, out: &mut Vec<Wall>| {
        let mut iter = list.into_iter();
        if let Some(mut cur) = iter.next() {
            for w in iter {
                if is_horizontal {
                    if (cur.z1 - w.z1).abs() < MERGE_EPS
                        && (cur.width - w.width).abs() < MERGE_EPS
                        && w.x1 <= cur.x2 + MERGE_EPS
                    {
                        cur.x2 = cur.x2.max(w.x2);
                        continue;
                    }
                } else if (cur.x1 - w.x1).abs() < MERGE_EPS
                    && (cur.width - w.width).abs() < MERGE_EPS
                    && w.z1 <= cur.z2 + MERGE_EPS
                {
                    cur.z2 = cur.z2.max(w.z2);
                    continue;
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
