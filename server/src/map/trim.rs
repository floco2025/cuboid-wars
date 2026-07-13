// Stacked-wall trim emission.
//
// A trim strip is a thin floor-thickness slab emitted at upper-level y to
// cap a wall stack — the gap between the top of a lower-level wall and the
// underside of the upper-level floor when both levels have a wall on the
// same grid edge but neither side has an upper-level floor slab covering
// the gap. The function below is one of three "trim" geometry sources in
// the map pipeline; the other two (perimeter extensions and corner fillers)
// live in `emit_floor_tier` and aren't moved out because they're tightly
// coupled to that function's per-cell state.

use super::{
    mask::Mask,
    segments::{horizontal_wall_segment, vertical_wall_segment},
};
use crate::map::EdgeGrid;
use common::{constants::FLOOR_THICKNESS, map::MapGeometry, protocol::Floor};

// Emit the thin trim strips that fill the gap between a stacked wall's top
// and the upper-level floor. A trim emits when both lower and upper levels
// have a wall on the same edge AND neither adjacent cell is in `upper_mask`
// (i.e. neither is an upper-level floor slab). Horizontal and vertical
// edges run their own pass because the iteration ranges, cell-pair offsets,
// and segment helpers differ.
#[must_use]
pub fn emit_stacked_wall_trim(
    lower_edges: &EdgeGrid,
    upper_edges: &EdgeGrid,
    upper_mask: &Mask,
    geometry: &MapGeometry,
    level: u8,
    y: f32,
) -> Vec<Floor> {
    let grid_cols = geometry.grid_cols;
    let grid_rows = geometry.grid_rows;
    let mut floors = Vec::new();
    let in_upper_mask =
        |r: i32, c: i32| r >= 0 && r < grid_rows && c >= 0 && c < grid_cols && upper_mask[r as usize][c as usize];

    // Horizontal pass: edge `horizontal[row][col]` separates cells
    // (row-1, col) and (row, col).
    for row in 0..=grid_rows {
        for col in 0..grid_cols {
            if !lower_edges.horizontal[row as usize][col as usize]
                || !upper_edges.horizontal[row as usize][col as usize]
                || in_upper_mask(row - 1, col)
                || in_upper_mask(row, col)
            {
                continue;
            }
            let lower = horizontal_wall_segment(lower_edges, row, col, geometry);
            let upper = horizontal_wall_segment(upper_edges, row, col, geometry);
            if let Some(segment) = lower.overlap(upper) {
                floors.push(segment.floor_strip(y, FLOOR_THICKNESS, level));
            }
        }
    }

    // Vertical pass: edge `vertical[row][col]` separates cells
    // (row, col-1) and (row, col).
    for row in 0..grid_rows {
        for col in 0..=grid_cols {
            if !lower_edges.vertical[row as usize][col as usize]
                || !upper_edges.vertical[row as usize][col as usize]
                || in_upper_mask(row, col - 1)
                || in_upper_mask(row, col)
            {
                continue;
            }
            let lower = vertical_wall_segment(lower_edges, row, col, geometry);
            let upper = vertical_wall_segment(upper_edges, row, col, geometry);
            if let Some(segment) = lower.overlap(upper) {
                floors.push(segment.floor_strip(y, FLOOR_THICKNESS, level));
            }
        }
    }

    floors
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::constants::{GRID_CELL_SIZE, LEVEL_HEIGHT, WALL_HALF_THICKNESS};

    fn empty_mask(cols: usize, rows: usize) -> Mask {
        vec![vec![false; cols]; rows]
    }

    #[test]
    fn stacked_horizontal_wall_emits_physical_trim_strip() {
        let mut lower_edges = EdgeGrid::new(1, 1);
        let mut upper_edges = EdgeGrid::new(1, 1);
        let upper_mask = empty_mask(1, 1);
        lower_edges.horizontal[1][0] = true;
        upper_edges.horizontal[1][0] = true;

        let geometry = MapGeometry::new(1, 1);
        let floors = emit_stacked_wall_trim(&lower_edges, &upper_edges, &upper_mask, &geometry, 1, LEVEL_HEIGHT);

        let half_w = geometry.width() / 2.0;
        let half_d = geometry.depth() / 2.0;
        assert_eq!(floors.len(), 1);
        assert_eq!(floors[0].x1, -half_w - WALL_HALF_THICKNESS);
        assert_eq!(floors[0].x2, -half_w + GRID_CELL_SIZE + WALL_HALF_THICKNESS);
        assert_eq!(floors[0].z1, -half_d + GRID_CELL_SIZE - WALL_HALF_THICKNESS);
        assert_eq!(floors[0].z2, -half_d + GRID_CELL_SIZE + WALL_HALF_THICKNESS);
        assert_eq!(floors[0].y, LEVEL_HEIGHT);
        assert_eq!(floors[0].thickness, FLOOR_THICKNESS);
        assert_eq!(floors[0].level, 1);
    }

    #[test]
    fn stacked_vertical_wall_emits_physical_trim_strip() {
        let mut lower_edges = EdgeGrid::new(1, 1);
        let mut upper_edges = EdgeGrid::new(1, 1);
        let upper_mask = empty_mask(1, 1);
        lower_edges.vertical[0][1] = true;
        upper_edges.vertical[0][1] = true;

        let geometry = MapGeometry::new(1, 1);
        let floors = emit_stacked_wall_trim(&lower_edges, &upper_edges, &upper_mask, &geometry, 1, LEVEL_HEIGHT);

        let half_w = geometry.width() / 2.0;
        let half_d = geometry.depth() / 2.0;
        assert_eq!(floors.len(), 1);
        assert_eq!(floors[0].x1, -half_w + GRID_CELL_SIZE - WALL_HALF_THICKNESS);
        assert_eq!(floors[0].x2, -half_w + GRID_CELL_SIZE + WALL_HALF_THICKNESS);
        assert_eq!(floors[0].z1, -half_d - WALL_HALF_THICKNESS);
        assert_eq!(floors[0].z2, -half_d + GRID_CELL_SIZE + WALL_HALF_THICKNESS);
        assert_eq!(floors[0].y, LEVEL_HEIGHT);
        assert_eq!(floors[0].thickness, FLOOR_THICKNESS);
        assert_eq!(floors[0].level, 1);
    }

    #[test]
    fn unstacked_wall_does_not_emit_trim_strip() {
        let mut lower_edges = EdgeGrid::new(1, 1);
        let upper_edges = EdgeGrid::new(1, 1);
        let upper_mask = empty_mask(1, 1);
        lower_edges.horizontal[1][0] = true;

        let geometry = MapGeometry::new(1, 1);
        let floors = emit_stacked_wall_trim(&lower_edges, &upper_edges, &upper_mask, &geometry, 1, LEVEL_HEIGHT);

        assert!(floors.is_empty());
    }

    #[test]
    fn stacked_wall_next_to_upper_floor_does_not_duplicate_trim() {
        let mut lower_edges = EdgeGrid::new(1, 1);
        let mut upper_edges = EdgeGrid::new(1, 1);
        let upper_mask = vec![vec![true]];
        lower_edges.horizontal[1][0] = true;
        upper_edges.horizontal[1][0] = true;
        lower_edges.vertical[0][1] = true;
        upper_edges.vertical[0][1] = true;

        let geometry = MapGeometry::new(1, 1);
        let floors = emit_stacked_wall_trim(&lower_edges, &upper_edges, &upper_mask, &geometry, 1, LEVEL_HEIGHT);

        assert!(floors.is_empty());
    }

    #[test]
    fn stacked_trim_uses_overlapping_wall_endpoint_rules() {
        let mut lower_edges = EdgeGrid::new(1, 2);
        let mut upper_edges = EdgeGrid::new(1, 2);
        let upper_mask = empty_mask(1, 2);
        lower_edges.horizontal[1][0] = true;
        upper_edges.horizontal[1][0] = true;
        upper_edges.vertical[0][0] = true;
        upper_edges.vertical[1][0] = true;

        let geometry = MapGeometry::new(1, 2);
        let floors = emit_stacked_wall_trim(&lower_edges, &upper_edges, &upper_mask, &geometry, 1, LEVEL_HEIGHT);

        let half_w = geometry.width() / 2.0;
        assert_eq!(floors.len(), 1);
        assert_eq!(floors[0].x1, -half_w + WALL_HALF_THICKNESS);
        assert_eq!(floors[0].x2, -half_w + GRID_CELL_SIZE + WALL_HALF_THICKNESS);
    }
}
