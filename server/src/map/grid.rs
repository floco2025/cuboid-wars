use common::{map::MapGeometry, protocol::Position};

// Convert a world position to grid coordinates
#[must_use]
pub fn grid_coords_from_position(geometry: &MapGeometry, pos: &Position) -> (i32, i32) {
    (
        geometry.cell_col_containing_x(pos.x),
        geometry.cell_row_containing_z(pos.z),
    )
}
