use super::edges::{CellSide, has_edge_on_cell_side};
use crate::resources::EdgeGrid;
use common::{map_geometry::MapGeometry, protocol::Position};

// Convert a world position to grid coordinates
#[must_use]
pub fn grid_coords_from_position(geometry: &MapGeometry, pos: &Position) -> (i32, i32) {
    (geometry.world_x_to_cell_col(pos.x), geometry.world_z_to_cell_row(pos.z))
}

// Count how many cell sides have edges (0-4).
#[allow(dead_code)]
pub(super) fn count_cell_side_edges(edges: &EdgeGrid, row: i32, col: i32) -> u8 {
    [CellSide::North, CellSide::South, CellSide::West, CellSide::East]
        .into_iter()
        .filter(|side| has_edge_on_cell_side(edges, row, col, *side))
        .count()
        .try_into()
        .expect("a cell has at most four edges")
}
