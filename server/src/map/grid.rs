use std::collections::{HashSet, VecDeque};

use super::{
    edges::{CellSide, has_edge_on_cell_side},
    mask::Mask,
};
use crate::resources::{CellGrid, EdgeGrid, LevelGrid};

// Check that every non-ramp cell within `mask` is reachable from a starting
// non-ramp masked cell. Used to reject wall placements that would split the
// playable area into disconnected regions. Currently unused.
#[allow(dead_code)]
pub fn all_cells_reachable_within_mask(level_grid: &LevelGrid, mask: &Mask, grid_cols: i32, grid_rows: i32) -> bool {
    all_cells_reachable_with_parts(&level_grid.cells, &level_grid.edges, mask, grid_cols, grid_rows)
}

fn all_cells_reachable_with_parts(
    cells: &CellGrid,
    edges: &EdgeGrid,
    mask: &Mask,
    grid_cols: i32,
    grid_rows: i32,
) -> bool {
    if grid_cols <= 0 || grid_rows <= 0 {
        return true;
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    let in_mask = |r: i32, c: i32| r >= 0 && r < grid_rows && c >= 0 && c < grid_cols && mask[r as usize][c as usize];

    let mut target_count = 0;
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            if in_mask(row, col) && !cells.rows[row as usize][col as usize].has_ramp {
                target_count += 1;
            }
        }
    }

    let mut start_found = false;
    'find_start: for row in 0..grid_rows {
        for col in 0..grid_cols {
            if in_mask(row, col) && !cells.rows[row as usize][col as usize].has_ramp {
                queue.push_back((row, col));
                visited.insert((row, col));
                start_found = true;
                break 'find_start;
            }
        }
    }

    if !start_found {
        return true;
    }

    while let Some((row, col)) = queue.pop_front() {
        let try_step =
            |dr: i32, dc: i32, has_wall: bool, visited: &mut HashSet<(i32, i32)>, queue: &mut VecDeque<(i32, i32)>| {
                let (nr, nc) = (row + dr, col + dc);
                if has_wall || !in_mask(nr, nc) || visited.contains(&(nr, nc)) {
                    return;
                }
                let next = &cells.rows[nr as usize][nc as usize];
                if next.has_ramp {
                    return;
                }
                visited.insert((nr, nc));
                queue.push_back((nr, nc));
            };

        try_step(
            -1,
            0,
            has_edge_on_cell_side(edges, row, col, CellSide::North),
            &mut visited,
            &mut queue,
        );
        try_step(
            1,
            0,
            has_edge_on_cell_side(edges, row, col, CellSide::South),
            &mut visited,
            &mut queue,
        );
        try_step(
            0,
            -1,
            has_edge_on_cell_side(edges, row, col, CellSide::West),
            &mut visited,
            &mut queue,
        );
        try_step(
            0,
            1,
            has_edge_on_cell_side(edges, row, col, CellSide::East),
            &mut visited,
            &mut queue,
        );

        if visited.len() == target_count {
            return true;
        }
    }

    visited.len() == target_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reachability_uses_horizontal_edge_grid() {
        let cells = CellGrid::new(1, 2);
        let mut edges = EdgeGrid::new(1, 2);
        edges.horizontal[1][0] = true;
        let mask = vec![vec![true], vec![true]];

        assert!(!all_cells_reachable_with_parts(&cells, &edges, &mask, 1, 2));
    }

    #[test]
    fn reachability_uses_vertical_edge_grid() {
        let cells = CellGrid::new(2, 1);
        let mut edges = EdgeGrid::new(2, 1);
        edges.vertical[0][1] = true;
        let mask = vec![vec![true, true]];

        assert!(!all_cells_reachable_with_parts(&cells, &edges, &mask, 2, 1));
    }
}
