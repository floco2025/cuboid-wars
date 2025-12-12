use rand::{Rng, rngs::ThreadRng};
use std::collections::{HashSet, VecDeque};

use crate::{
    constants::{
        ROOF_OVERLAP_MODE, ROOF_PROBABILITY_2_WALLS, ROOF_PROBABILITY_3_WALLS, ROOF_PROBABILITY_WITH_NEIGHBOR,
        WALL_2ND_PROBABILITY_RATIO, WALL_3RD_PROBABILITY_RATIO, WALL_NUM_SEGMENTS, WALL_OVERLAP_MODE,
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
#[allow(clippy::too_many_lines)]
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
    let walls = generate_individual_walls(&grid, grid_cols, grid_rows);

    // Generate roofs based on grid
    let roofs = generate_individual_roofs(&grid, grid_cols, grid_rows);

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
    let check_col = (col - 1).clamp(0, grid_cols - 1);
    let edge_col = col.min(grid_cols - 1);

    let has_perp_top = row > 0
        && (grid[(row - 1) as usize][check_col as usize].has_south_wall
            || grid[row as usize][edge_col as usize].has_north_wall);

    let has_perp_bottom = row < grid_rows - 1
        && (grid[row as usize][check_col as usize].has_south_wall
            || grid[(row + 1) as usize][edge_col as usize].has_north_wall);

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

            let world_z = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            let x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0))
                - if WALL_OVERLAP_MODE || !has_left {
                    WALL_WIDTH / 2.0
                } else {
                    0.0
                };
            let x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0))
                + if WALL_OVERLAP_MODE || !has_right {
                    WALL_WIDTH / 2.0
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
        let (world_x1, world_x2, world_z1, world_z2) = if ROOF_OVERLAP_MODE {
            // Overlap mode: extend on all sides by roof_thickness/2 for guaranteed coverage
            let x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)) - WALL_WIDTH / 2.0;
            let x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0)) + WALL_WIDTH / 2.0;
            let z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)) - WALL_WIDTH / 2.0;
            let z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0)) + WALL_WIDTH / 2.0;
            (x1, x2, z1, z2)
        } else {
            // Non-overlap mode: extend to grid boundaries to cover full wall length
            let x1 = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let x2 = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let z1 = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            let z2 = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
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

// Check if all cells in the grid are reachable from cell (0, 0)
