use rand::Rng;
use std::collections::{HashSet, VecDeque};

use crate::{
    constants::{NUM_WALL_SEGMENTS, ROOF_PROBABILITY_2_WALLS, ROOF_PROBABILITY_3_WALLS, ROOF_PROBABILITY_WITH_NEIGHBOR, WALL_2ND_PROBABILITY_RATIO, WALL_3RD_PROBABILITY_RATIO},
    resources::{GridCell, GridConfig},
};
use common::{
    constants::*,
    protocol::{Roof, Wall, WallOrientation},
};

// Helper to count walls in a cell
fn count_cell_walls(cell: &GridCell) -> u8 {
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
    }

    // All cells should be reachable
    #[allow(clippy::cast_sign_loss)]
    {
        visited.len() == (grid_rows * grid_cols) as usize
    }
}

// Generate grid configuration for the playing field.
//
// Walls are placed along grid lines in a maze-like pattern.
// Ensures all grid cells remain reachable from each other.
// Always places walls around the perimeter of the field.
// Returns a complete GridConfig with walls, roofs, and grid cell data.
#[must_use]
#[allow(clippy::too_many_lines)]
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
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
        if interior_walls_placed >= NUM_WALL_SEGMENTS {
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
        let cell1_walls = count_cell_walls(cell);
        let cell2_walls = match direction {
            0 => {
                // South wall - check cell below
                if row < grid_rows - 1 {
                    count_cell_walls(&grid[(row + 1) as usize][col as usize])
                } else {
                    0
                }
            },
            1 => {
                // East wall - check cell to the right
                if col < grid_cols - 1 {
                    count_cell_walls(&grid[row as usize][(col + 1) as usize])
                } else {
                    0
                }
            },
            _ => 0,
        };
        
        let max_walls = cell1_walls.max(cell2_walls);

        // Apply probability based on existing wall count
        let ratio = match max_walls {
            0 => 1.0,
            1 => WALL_2ND_PROBABILITY_RATIO,
            2 => WALL_3RD_PROBABILITY_RATIO,
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
            },
            1 => {
                grid[row as usize][col as usize].has_east_wall = true;
                if col < grid_cols - 1 {
                    grid[row as usize][(col + 1) as usize].has_west_wall = true;
                }
            },
            _ => {},
        }

        // Check if all cells are still reachable
        if !all_cells_reachable(&grid, grid_cols, grid_rows) {
            // Remove the wall
            match direction {
                0 => {
                    grid[row as usize][col as usize].has_south_wall = false;
                    if row < grid_rows - 1 {
                        grid[(row + 1) as usize][col as usize].has_north_wall = false;
                    }
                },
                1 => {
                    grid[row as usize][col as usize].has_east_wall = false;
                    if col < grid_cols - 1 {
                        grid[row as usize][(col + 1) as usize].has_west_wall = false;
                    }
                },
                _ => {},
            }
        } else {
            interior_walls_placed += 1;
        }
    }

    // Build wall list from grid
    let mut walls = Vec::new();
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let cell = &grid[row as usize][col as usize];
            
            // North wall (horizontal)
            if cell.has_north_wall {
                let world_x = (col as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
                let world_z = (row as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
                walls.push(Wall {
                    x: world_x,
                    z: world_z,
                    orientation: WallOrientation::Horizontal,
                });
            }
            
            // South wall (horizontal) - only if it's the last row or neighbor doesn't have it
            if cell.has_south_wall && (row == grid_rows - 1 || !grid[(row + 1) as usize][col as usize].has_north_wall) {
                let world_x = (col as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
                let world_z = ((row + 1) as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
                walls.push(Wall {
                    x: world_x,
                    z: world_z,
                    orientation: WallOrientation::Horizontal,
                });
            }
            
            // West wall (vertical)
            if cell.has_west_wall {
                let world_x = (col as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
                let world_z = (row as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
                walls.push(Wall {
                    x: world_x,
                    z: world_z,
                    orientation: WallOrientation::Vertical,
                });
            }
            
            // East wall (vertical) - only if it's the last column or neighbor doesn't have it
            if cell.has_east_wall && (col == grid_cols - 1 || !grid[row as usize][(col + 1) as usize].has_west_wall) {
                let world_x = ((col + 1) as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
                let world_z = (row as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
                walls.push(Wall {
                    x: world_x,
                    z: world_z,
                    orientation: WallOrientation::Vertical,
                });
            }
        }
    }

    // Generate roofs based on grid
    let roofs = generate_roofs_from_grid(&grid, grid_cols, grid_rows);
    
    GridConfig { walls, roofs, grid }
}

// Generate roofs based on wall count in each cell, with two-pass algorithm
// Pass 1: Place roofs based on wall count
// Pass 2: Place additional roofs adjacent to existing ones
#[must_use]
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
fn generate_roofs_from_grid(grid: &[Vec<GridCell>], grid_cols: i32, grid_rows: i32) -> Vec<Roof> {
    let mut rng = rand::rng();

    // Count walls for each cell
    let mut wall_counts = vec![vec![0u8; grid_cols as usize]; grid_rows as usize];
    
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let cell = &grid[row as usize][col as usize];
            let mut wall_count = 0;
            
            if cell.has_north_wall {
                wall_count += 1;
            }
            if cell.has_south_wall {
                wall_count += 1;
            }
            if cell.has_west_wall {
                wall_count += 1;
            }
            if cell.has_east_wall {
                wall_count += 1;
            }
            
            wall_counts[row as usize][col as usize] = wall_count;
        }
    }

    // Pass 1: Place roofs based on wall count (no neighbor consideration)
    let mut roof_cells: HashSet<(i32, i32)> = HashSet::new();
    
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let wall_count = wall_counts[row as usize][col as usize];
            
            let should_place_roof = match wall_count {
                2 => rng.random_bool(ROOF_PROBABILITY_2_WALLS),
                3 => rng.random_bool(ROOF_PROBABILITY_3_WALLS),
                _ => false, // 0, 1, or 4 walls: no roof
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
                // Skip if already has roof or not enough walls
                if roof_cells.contains(&(row, col)) || wall_counts[row as usize][col as usize] < 2 {
                    continue;
                }
                
                // Check if any neighbor has a roof
                let neighbors = [
                    (row - 1, col), // North
                    (row + 1, col), // South
                    (row, col - 1), // West
                    (row, col + 1), // East
                ];
                
                let has_neighbor_with_roof = neighbors.iter().any(|&(r, c)| {
                    r >= 0 && r < grid_rows && c >= 0 && c < grid_cols && roof_cells.contains(&(r, c))
                });
                
                if has_neighbor_with_roof && rng.random_bool(ROOF_PROBABILITY_WITH_NEIGHBOR) {
                    roof_cells.insert((row, col));
                    added_more = true;
                }
            }
        }
    }

    // Convert to Roof structs
    roof_cells
        .into_iter()
        .map(|(row, col)| Roof {
            row: row as u32,
            col: col as u32,
        })
        .collect()
}
