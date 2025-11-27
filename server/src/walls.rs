use common::{
    constants::*,
    protocol::{Wall, WallOrientation},
};
use rand::Rng;
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GridEdge {
    x: i32,      // Grid line position
    z: i32,      // Grid line position
    horizontal: bool, // true = horizontal (along X), false = vertical (along Z)
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
            (row - 1, col, GridEdge { x: col, z: row, horizontal: true }),     // North
            (row + 1, col, GridEdge { x: col, z: row + 1, horizontal: true }), // South
            (row, col - 1, GridEdge { x: col, z: row, horizontal: false }),    // West
            (row, col + 1, GridEdge { x: col + 1, z: row, horizontal: false }), // East
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

/// Generate wall segments for the playing field.
/// Walls are placed along grid lines in a maze-like pattern.
/// Ensures all grid cells remain reachable from each other.
pub fn generate_walls() -> Vec<Wall> {
    let mut rng = rand::rng();
    let mut walls = Vec::new();
    let mut placed_edges: HashSet<GridEdge> = HashSet::new();
    
    // Calculate grid dimensions
    let grid_cols = (FIELD_WIDTH / GRID_SIZE) as i32;
    let grid_rows = (FIELD_DEPTH / GRID_SIZE) as i32;
    
    // Generate all possible edge positions
    let mut all_edges = Vec::new();
    
    // Horizontal edges (along X axis)
    for z in 0..=grid_rows {
        for x in 0..grid_cols {
            all_edges.push(GridEdge { x, z, horizontal: true });
        }
    }
    
    // Vertical edges (along Z axis)
    for z in 0..grid_rows {
        for x in 0..=grid_cols {
            all_edges.push(GridEdge { x, z, horizontal: false });
        }
    }
    
    // Shuffle the edges randomly
    for i in (1..all_edges.len()).rev() {
        let j = rng.random_range(0..=i);
        all_edges.swap(i, j);
    }
    
    // Try to place walls at each edge position
    for edge in all_edges {
        if walls.len() >= NUM_WALL_SEGMENTS {
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
            let world_x = (x as f32 + 0.5) * GRID_SIZE - FIELD_WIDTH / 2.0;
            let world_z = z as f32 * GRID_SIZE - FIELD_DEPTH / 2.0;
            
            walls.push(Wall {
                x: world_x,
                z: world_z,
                orientation: WallOrientation::Horizontal,
            });
        } else {
            let world_x = x as f32 * GRID_SIZE - FIELD_WIDTH / 2.0;
            let world_z = (z as f32 + 0.5) * GRID_SIZE - FIELD_DEPTH / 2.0;
            
            walls.push(Wall {
                x: world_x,
                z: world_z,
                orientation: WallOrientation::Vertical,
            });
        }
    }
    
    walls
}
