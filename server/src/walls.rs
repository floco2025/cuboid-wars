use common::{
    constants::*,
    protocol::{Wall, WallOrientation},
};
use rand::Rng;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GridEdge {
    x: i32,      // Grid line position
    z: i32,      // Grid line position
    horizontal: bool, // true = horizontal (along X), false = vertical (along Z)
}

/// Generate wall segments for the playing field.
/// Walls are placed along grid lines in a maze-like pattern.
/// Ensures no grid cell has more than 3 walls (to keep it accessible).
pub fn generate_walls() -> Vec<Wall> {
    let mut rng = rand::rng();
    let mut walls = Vec::new();
    let mut placed_edges: HashSet<GridEdge> = HashSet::new();
    
    // Calculate grid dimensions
    let grid_cols = (FIELD_WIDTH / GRID_SIZE) as i32;
    let grid_rows = (FIELD_DEPTH / GRID_SIZE) as i32;
    
    // Track wall count per grid cell (row, col) -> count
    let mut cell_wall_count: Vec<Vec<u8>> = vec![vec![0; grid_cols as usize]; grid_rows as usize];
    
    let mut attempts = 0;
    let max_attempts = NUM_WALL_SEGMENTS * 10; // Prevent infinite loop
    
    while walls.len() < NUM_WALL_SEGMENTS && attempts < max_attempts {
        attempts += 1;
        
        // Pick random orientation
        let horizontal = rng.random_bool(0.5);
        
        // Pick random grid line position
        let (x, z) = if horizontal {
            // Horizontal wall (along X axis, on Z grid line)
            let z_line = rng.random_range(0..=grid_rows);
            let x_line = rng.random_range(0..grid_cols);
            (x_line, z_line)
        } else {
            // Vertical wall (along Z axis, on X grid line)
            let x_line = rng.random_range(0..=grid_cols);
            let z_line = rng.random_range(0..grid_rows);
            (x_line, z_line)
        };
        
        let edge = GridEdge { x, z, horizontal };
        
        // Skip if already placed
        if placed_edges.contains(&edge) {
            continue;
        }
        
        // Check if placing this wall would make any adjacent cell have more than 3 walls
        if horizontal {
            // Horizontal wall affects cells above (z-1) and below (z)
            let mut valid = true;
            
            // Cell below (z)
            if z < grid_rows {
                let row = z as usize;
                let col = x as usize;
                if cell_wall_count[row][col] >= 3 {
                    valid = false;
                }
            }
            
            // Cell above (z-1)
            if z > 0 {
                let row = (z - 1) as usize;
                let col = x as usize;
                if cell_wall_count[row][col] >= 3 {
                    valid = false;
                }
            }
            
            if !valid {
                continue;
            }
            
            // Place the wall
            placed_edges.insert(edge);
            
            // Update wall counts
            if z < grid_rows {
                cell_wall_count[z as usize][x as usize] += 1;
            }
            if z > 0 {
                cell_wall_count[(z - 1) as usize][x as usize] += 1;
            }
            
            // Calculate world position (center of wall)
            let world_x = (x as f32 + 0.5) * GRID_SIZE - FIELD_WIDTH / 2.0;
            let world_z = z as f32 * GRID_SIZE - FIELD_DEPTH / 2.0;
            
            walls.push(Wall {
                x: world_x,
                z: world_z,
                orientation: WallOrientation::Horizontal,
            });
        } else {
            // Vertical wall affects cells left (x-1) and right (x)
            let mut valid = true;
            
            // Cell to the right (x)
            if x < grid_cols {
                let row = z as usize;
                let col = x as usize;
                if cell_wall_count[row][col] >= 3 {
                    valid = false;
                }
            }
            
            // Cell to the left (x-1)
            if x > 0 {
                let row = z as usize;
                let col = (x - 1) as usize;
                if cell_wall_count[row][col] >= 3 {
                    valid = false;
                }
            }
            
            if !valid {
                continue;
            }
            
            // Place the wall
            placed_edges.insert(edge);
            
            // Update wall counts
            if x < grid_cols {
                cell_wall_count[z as usize][x as usize] += 1;
            }
            if x > 0 {
                cell_wall_count[z as usize][(x - 1) as usize] += 1;
            }
            
            // Calculate world position (center of wall)
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
