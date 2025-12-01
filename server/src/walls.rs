use rand::Rng;
use std::collections::{HashSet, VecDeque};

use crate::constants::{NUM_WALL_SEGMENTS, ROOF_PROBABILITY_2_WALLS, ROOF_PROBABILITY_3_WALLS, ROOF_PROBABILITY_WITH_NEIGHBOR, WALL_2ND_PROBABILITY_RATIO, WALL_3RD_PROBABILITY_RATIO};
use common::{
    constants::*,
    protocol::{Roof, Wall, WallOrientation},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GridEdge {
    x: i32,           // Grid line position
    z: i32,           // Grid line position
    horizontal: bool, // true = horizontal (along X), false = vertical (along Z)
}

// Check if all grid cells are reachable using BFS
fn all_cells_reachable(placed_edges: &HashSet<GridEdge>, grid_cols: i32, grid_rows: i32) -> bool {
    if grid_cols <= 0 || grid_rows <= 0 {
        return true;
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    // Start from cell (0, 0)
    queue.push_back((0, 0));
    visited.insert((0, 0));

    while let Some((row, col)) = queue.pop_front() {
        // Check all 4 directions
        let directions = [
            (
                row - 1,
                col,
                GridEdge {
                    x: col,
                    z: row,
                    horizontal: true,
                },
            ), // North
            (
                row + 1,
                col,
                GridEdge {
                    x: col,
                    z: row + 1,
                    horizontal: true,
                },
            ), // South
            (
                row,
                col - 1,
                GridEdge {
                    x: col,
                    z: row,
                    horizontal: false,
                },
            ), // West
            (
                row,
                col + 1,
                GridEdge {
                    x: col + 1,
                    z: row,
                    horizontal: false,
                },
            ), // East
        ];

        for (new_row, new_col, edge) in directions {
            // Check bounds
            if new_row < 0 || new_row >= grid_rows || new_col < 0 || new_col >= grid_cols {
                continue;
            }

            // Check if already visited
            if visited.contains(&(new_row, new_col)) {
                continue;
            }

            // Check if wall blocks this direction
            if placed_edges.contains(&edge) {
                continue;
            }

            // Can reach this cell
            visited.insert((new_row, new_col));
            queue.push_back((new_row, new_col));
        }
    }

    // All cells should be reachable
    #[allow(clippy::cast_sign_loss)]
    {
        visited.len() == (grid_rows * grid_cols) as usize
    }
}

// Generate wall segments for the playing field.
//
// Walls are placed along grid lines in a maze-like pattern.
// Ensures all grid cells remain reachable from each other.
// Always places walls around the perimeter of the field.
#[must_use]
#[allow(clippy::too_many_lines)]
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
pub fn generate_walls() -> Vec<Wall> {
    let mut rng = rand::rng();
    let mut walls = Vec::new();
    let mut placed_edges: HashSet<GridEdge> = HashSet::new();

    // Calculate grid dimensions
    let grid_cols = (FIELD_WIDTH / GRID_SIZE) as i32;
    let grid_rows = (FIELD_DEPTH / GRID_SIZE) as i32;

    // First, place all perimeter walls
    // Top edge (z = 0)
    for x in 0..grid_cols {
        let edge = GridEdge {
            x,
            z: 0,
            horizontal: true,
        };
        placed_edges.insert(edge);
        let world_x = (x as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let world_z = 0.0f32.mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
        walls.push(Wall {
            x: world_x,
            z: world_z,
            orientation: WallOrientation::Horizontal,
        });
    }

    // Bottom edge (z = grid_rows)
    for x in 0..grid_cols {
        let edge = GridEdge {
            x,
            z: grid_rows,
            horizontal: true,
        };
        placed_edges.insert(edge);
        let world_x = (x as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let world_z = (grid_rows as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
        walls.push(Wall {
            x: world_x,
            z: world_z,
            orientation: WallOrientation::Horizontal,
        });
    }

    // Left edge (x = 0)
    for z in 0..grid_rows {
        let edge = GridEdge {
            x: 0,
            z,
            horizontal: false,
        };
        placed_edges.insert(edge);
        let world_x = 0.0f32.mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let world_z = (z as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
        walls.push(Wall {
            x: world_x,
            z: world_z,
            orientation: WallOrientation::Vertical,
        });
    }

    // Right edge (x = grid_cols)
    for z in 0..grid_rows {
        let edge = GridEdge {
            x: grid_cols,
            z,
            horizontal: false,
        };
        placed_edges.insert(edge);
        let world_x = (grid_cols as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let world_z = (z as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
        walls.push(Wall {
            x: world_x,
            z: world_z,
            orientation: WallOrientation::Vertical,
        });
    }

    // Generate all possible interior edge positions
    let mut all_edges = Vec::new();

    // Interior horizontal edges (along X axis)
    for z in 1..grid_rows {
        for x in 0..grid_cols {
            all_edges.push(GridEdge { x, z, horizontal: true });
        }
    }

    // Interior vertical edges (along Z axis)
    for z in 0..grid_rows {
        for x in 1..grid_cols {
            all_edges.push(GridEdge {
                x,
                z,
                horizontal: false,
            });
        }
    }

    // Shuffle the edges randomly
    for i in (1..all_edges.len()).rev() {
        let j = rng.random_range(0..=i);
        all_edges.swap(i, j);
    }

    // Try to place walls at each interior edge position
    for edge in all_edges {
        // Only count interior walls toward NUM_WALL_SEGMENTS
        let interior_walls_count = walls.len() - (2 * grid_cols as usize + 2 * grid_rows as usize);
        if interior_walls_count >= NUM_WALL_SEGMENTS {
            break;
        }

        // Skip if already placed (shouldn't happen with our generation, but be safe)
        if placed_edges.contains(&edge) {
            continue;
        }

        // Count walls in the two cells adjacent to this edge
        let (cell1_row, cell1_col, cell2_row, cell2_col) = if edge.horizontal {
            // Horizontal edge: cells above (z-1) and below (z)
            (edge.z - 1, edge.x, edge.z, edge.x)
        } else {
            // Vertical edge: cells left (x-1) and right (x)
            (edge.z, edge.x - 1, edge.z, edge.x)
        };

        let mut max_walls = 0;
        
        // Count walls for cell 1 (if in bounds)
        if cell1_row >= 0 && cell1_row < grid_rows && cell1_col >= 0 && cell1_col < grid_cols {
            let mut wall_count = 0;
            // Check all 4 edges of this cell
            if placed_edges.contains(&GridEdge { x: cell1_col, z: cell1_row, horizontal: true }) {
                wall_count += 1;
            }
            if placed_edges.contains(&GridEdge { x: cell1_col, z: cell1_row + 1, horizontal: true }) {
                wall_count += 1;
            }
            if placed_edges.contains(&GridEdge { x: cell1_col, z: cell1_row, horizontal: false }) {
                wall_count += 1;
            }
            if placed_edges.contains(&GridEdge { x: cell1_col + 1, z: cell1_row, horizontal: false }) {
                wall_count += 1;
            }
            max_walls = max_walls.max(wall_count);
        }

        // Count walls for cell 2 (if in bounds)
        if cell2_row >= 0 && cell2_row < grid_rows && cell2_col >= 0 && cell2_col < grid_cols {
            let mut wall_count = 0;
            // Check all 4 edges of this cell
            if placed_edges.contains(&GridEdge { x: cell2_col, z: cell2_row, horizontal: true }) {
                wall_count += 1;
            }
            if placed_edges.contains(&GridEdge { x: cell2_col, z: cell2_row + 1, horizontal: true }) {
                wall_count += 1;
            }
            if placed_edges.contains(&GridEdge { x: cell2_col, z: cell2_row, horizontal: false }) {
                wall_count += 1;
            }
            if placed_edges.contains(&GridEdge { x: cell2_col + 1, z: cell2_row, horizontal: false }) {
                wall_count += 1;
            }
            max_walls = max_walls.max(wall_count);
        }

        // Apply probability based on existing wall count
        // max_walls: 0 = first wall, 1 = second wall, 2 = third wall, 3+ = fourth wall
        // First wall is always attempted (probability 1.0); others use ratio relative to first
        let ratio = match max_walls {
            0 => 1.0, // First wall - always attempt (subject to connectivity)
            1 => WALL_2ND_PROBABILITY_RATIO,
            2 => WALL_3RD_PROBABILITY_RATIO,
            _ => WALL_3RD_PROBABILITY_RATIO, // Fourth wall uses same as third
        };

        // Skip this wall based on ratio (ratios > 1.0 mean always accept)
        if ratio < 1.0 && !rng.random_bool(ratio) {
            continue;
        }

        // Temporarily place the wall and check if all cells are still reachable
        placed_edges.insert(edge);

        if !all_cells_reachable(&placed_edges, grid_cols, grid_rows) {
            // This wall would block connectivity - remove it and try next position
            placed_edges.remove(&edge);
            continue;
        }

        // Wall is valid - calculate world position and add to list
        let (x, z, horizontal) = (edge.x, edge.z, edge.horizontal);

        if horizontal {
            let world_x = (x as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let world_z = (z as f32).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

            walls.push(Wall {
                x: world_x,
                z: world_z,
                orientation: WallOrientation::Horizontal,
            });
        } else {
            let world_x = (x as f32).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let world_z = (z as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

            walls.push(Wall {
                x: world_x,
                z: world_z,
                orientation: WallOrientation::Vertical,
            });
        }
    }

    walls
}

// Generate roofs based on wall count in each cell, with two-pass algorithm
// Pass 1: Place roofs based on wall count
// Pass 2: Place additional roofs adjacent to existing ones
#[must_use]
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
pub fn generate_roofs(walls: &[Wall]) -> Vec<Roof> {
    let mut rng = rand::rng();
    let grid_cols = (FIELD_WIDTH / GRID_SIZE) as i32;
    let grid_rows = (FIELD_DEPTH / GRID_SIZE) as i32;

    // Convert walls to GridEdge representation for easier counting
    let mut placed_edges: HashSet<GridEdge> = HashSet::new();
    
    for wall in walls {
        // Convert world position back to grid position
        let grid_x = ((wall.x + FIELD_WIDTH / 2.0) / GRID_SIZE) as i32;
        let grid_z = ((wall.z + FIELD_DEPTH / 2.0) / GRID_SIZE) as i32;
        
        let edge = match wall.orientation {
            WallOrientation::Horizontal => GridEdge {
                x: grid_x,
                z: grid_z,
                horizontal: true,
            },
            WallOrientation::Vertical => GridEdge {
                x: grid_x,
                z: grid_z,
                horizontal: false,
            },
        };
        placed_edges.insert(edge);
    }

    // Count walls for each cell
    let mut wall_counts = vec![vec![0u8; grid_cols as usize]; grid_rows as usize];
    
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let mut wall_count = 0;
            
            // Check north edge
            if placed_edges.contains(&GridEdge {
                x: col,
                z: row,
                horizontal: true,
            }) {
                wall_count += 1;
            }
            
            // Check south edge
            if placed_edges.contains(&GridEdge {
                x: col,
                z: row + 1,
                horizontal: true,
            }) {
                wall_count += 1;
            }
            
            // Check west edge
            if placed_edges.contains(&GridEdge {
                x: col,
                z: row,
                horizontal: false,
            }) {
                wall_count += 1;
            }
            
            // Check east edge
            if placed_edges.contains(&GridEdge {
                x: col + 1,
                z: row,
                horizontal: false,
            }) {
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
