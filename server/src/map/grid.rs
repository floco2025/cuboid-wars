use std::collections::{HashSet, VecDeque};

use crate::resources::GridCell;

// Check if all non-ramp cells are reachable from each other
pub fn all_cells_reachable(grid: &[Vec<GridCell>], grid_cols: i32, grid_rows: i32) -> bool {
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
