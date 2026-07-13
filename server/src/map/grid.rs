use common::{map::MapGeometry, protocol::Position};

// Convert a world position to grid coordinates
#[must_use]
pub fn grid_coords_from_position(geometry: &MapGeometry, pos: &Position) -> (i32, i32) {
    (geometry.world_x_to_cell_col(pos.x), geometry.world_z_to_cell_row(pos.z))
}
