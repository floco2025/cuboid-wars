// Wall trim emission.
//
// A trim strip is a thin floor-thickness slab emitted at upper-level y
// under an upper-level wall or window (`band_edges`): the floor band
// between the top of the storey below and the wall's base, which a floor
// slab fills where one lies beside the edge. Without the strip a wall over
// a lower wall, a doorway, or open air would hang a slab's thickness short,
// and a shot or a line of sight would pass under it. The functions below
// are one of three "trim" geometry
// sources in the map pipeline; the other two (perimeter extensions and
// corner fillers) live in `emit_floor_tier` and aren't moved out because
// they're tightly coupled to that function's per-cell state.

use super::{
    mask::Mask,
    segments::{horizontal_wall_segment, vertical_wall_segment},
};
use crate::map::EdgeGrid;
use common::{
    map::MapGeometry,
    protocol::{CarrierId, Floor},
};

// The edges whose band gets a strip: the upper storey's walls, and its
// barriers that stand on a lower wall. A barrier is a wall section tall, so
// over a wall the band would stay open under the field; over a lower
// barrier the two fields stack into one and a strip would cut the opening.
#[must_use]
pub fn band_edges(lower_walls: &EdgeGrid, upper_walls: &EdgeGrid, upper_barriers: &EdgeGrid) -> EdgeGrid {
    fn merge(lower_walls: &[Vec<bool>], upper_walls: &[Vec<bool>], upper_barriers: &[Vec<bool>]) -> Vec<Vec<bool>> {
        lower_walls
            .iter()
            .zip(upper_walls)
            .zip(upper_barriers)
            .map(|((lower, upper), barriers)| {
                lower
                    .iter()
                    .zip(upper)
                    .zip(barriers)
                    .map(|((lower, upper), barrier)| *upper || (*barrier && *lower))
                    .collect()
            })
            .collect()
    }
    EdgeGrid {
        horizontal: merge(
            &lower_walls.horizontal,
            &upper_walls.horizontal,
            &upper_barriers.horizontal,
        ),
        vertical: merge(&lower_walls.vertical, &upper_walls.vertical, &upper_barriers.vertical),
    }
}

// Emit the thin trim strips under the upper level's band edges. A trim
// emits for every edge in `band` where neither adjacent cell is in
// `upper_mask` (i.e. neither is an upper-level floor slab), spanning the
// edge's own segment with its corner extensions. Horizontal and vertical
// edges run their own pass because the iteration ranges, cell-pair offsets,
// and segment helpers differ.
#[must_use]
pub fn emit_wall_trim(
    band: &EdgeGrid,
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
            if !band.horizontal[row as usize][col as usize] || in_upper_mask(row - 1, col) || in_upper_mask(row, col) {
                continue;
            }
            floors.push(horizontal_wall_segment(band, row, col, geometry).floor_strip(
                y,
                geometry.floor_thickness(),
                geometry.wall_half_thickness(),
                level,
                carrier,
            ));
        }
    }

    // Vertical pass: edge `vertical[row][col]` separates cells
    // (row, col-1) and (row, col).
    for row in 0..grid_rows {
        for col in 0..=grid_cols {
            if !band.vertical[row as usize][col as usize] || in_upper_mask(row, col - 1) || in_upper_mask(row, col) {
                continue;
            }
            floors.push(vertical_wall_segment(band, row, col, geometry).floor_strip(
                y,
                geometry.floor_thickness(),
                geometry.wall_half_thickness(),
                level,
                carrier,
            ));
        }
    }

    floors
}

#[cfg(test)]
#[path = "tests/trim.rs"]
mod tests;
