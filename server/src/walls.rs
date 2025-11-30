use rand::Rng;
use std::collections::{HashSet, VecDeque};

use crate::constants::NUM_WALL_SEGMENTS;
use common::{
    constants::*,
    protocol::{Wall, WallOrientation},
};

#[derive(Debug, Clone, Copy, PartialEq)]
struct GridEdge {
    x: f32,           // Grid line position
    z: f32,           // Grid line position
    horizontal: bool, // true = horizontal (along X), false = vertical (along Z)
}

impl Eq for GridEdge {}

impl std::hash::Hash for GridEdge {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.x.to_bits().hash(state);
        self.z.to_bits().hash(state);
        self.horizontal.hash(state);
    }
}

/// Check if all grid cells are reachable using BFS
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
                    x: col as f32,
                    z: row as f32,
                    horizontal: true,
                },
            ), // North
            (
                row + 1,
                col,
                GridEdge {
                    x: col as f32,
                    z: (row + 1) as f32,
                    horizontal: true,
                },
            ), // South
            (
                row,
                col - 1,
                GridEdge {
                    x: col as f32,
                    z: row as f32,
                    horizontal: false,
                },
            ), // West
            (
                row,
                col + 1,
                GridEdge {
                    x: (col + 1) as f32,
                    z: row as f32,
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
    visited.len() == (grid_rows * grid_cols) as usize
}

// Generate wall segments for the playing field.
//
// Walls are placed along grid lines in a maze-like pattern.
// Ensures all grid cells remain reachable from each other.
// Always places walls around the perimeter of the field.
#[must_use]
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
            x: x as f32,
            z: 0.0,
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
            x: x as f32,
            z: grid_rows as f32,
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
            x: 0.0,
            z: z as f32,
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
            x: grid_cols as f32,
            z: z as f32,
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
            all_edges.push(GridEdge { x: x as f32, z: z as f32, horizontal: true });
        }
    }

    // Interior vertical edges (along Z axis)
    for z in 0..grid_rows {
        for x in 1..grid_cols {
            all_edges.push(GridEdge {
                x: x as f32,
                z: z as f32,
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
            let world_x = (x + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let world_z = z.mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

            walls.push(Wall {
                x: world_x,
                z: world_z,
                orientation: WallOrientation::Horizontal,
            });
        } else {
            let world_x = x.mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let world_z = (z + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

            walls.push(Wall {
                x: world_x,
                z: world_z,
                orientation: WallOrientation::Vertical,
            });
        }
    }

    walls
}
