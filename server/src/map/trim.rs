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
use common::{
    map::MapGeometry,
    protocol::{CarrierId, Floor},
};

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
    carrier: CarrierId,
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
                floors.push(segment.floor_strip(
                    y,
                    geometry.floor_thickness(),
                    geometry.wall_half_thickness(),
                    level,
                    carrier,
                ));
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
                floors.push(segment.floor_strip(
                    y,
                    geometry.floor_thickness(),
                    geometry.wall_half_thickness(),
                    level,
                    carrier,
                ));
            }
        }
    }

    floors
}

#[cfg(test)]
#[path = "tests/trim.rs"]
mod tests;
