use bevy_ecs::prelude::Resource;

use crate::constants::GRID_CELL_SIZE;

#[derive(Debug, Clone, Copy, Resource)]
pub struct MapGeometry {
    pub grid_cols: i32,
    pub grid_rows: i32,
}

impl MapGeometry {
    #[must_use]
    pub const fn new(grid_cols: i32, grid_rows: i32) -> Self {
        Self { grid_cols, grid_rows }
    }

    #[must_use]
    pub fn width(&self) -> f32 {
        self.grid_cols as f32 * GRID_CELL_SIZE
    }

    #[must_use]
    pub fn depth(&self) -> f32 {
        self.grid_rows as f32 * GRID_CELL_SIZE
    }

    #[must_use]
    pub fn cell_to_world_x(&self, col: i32) -> f32 {
        (col as f32).mul_add(GRID_CELL_SIZE, -(self.width() / 2.0))
    }

    #[must_use]
    pub fn cell_to_world_z(&self, row: i32) -> f32 {
        (row as f32).mul_add(GRID_CELL_SIZE, -(self.depth() / 2.0))
    }

    #[must_use]
    pub fn cell_col_containing_x(&self, x: f32) -> i32 {
        ((x + self.width() / 2.0) / GRID_CELL_SIZE).floor() as i32
    }

    #[must_use]
    pub fn cell_row_containing_z(&self, z: f32) -> i32 {
        ((z + self.depth() / 2.0) / GRID_CELL_SIZE).floor() as i32
    }

    #[must_use]
    pub fn nearest_grid_col_to_x(&self, x: f32) -> i32 {
        ((x + self.width() / 2.0) / GRID_CELL_SIZE).round() as i32
    }

    #[must_use]
    pub fn nearest_grid_row_to_z(&self, z: f32) -> i32 {
        ((z + self.depth() / 2.0) / GRID_CELL_SIZE).round() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_center_round_trips_to_the_same_cell() {
        let geometry = MapGeometry::new(30, 20);
        for col in [0, 1, 15, 29] {
            let center_x = geometry.cell_to_world_x(col) + GRID_CELL_SIZE / 2.0;
            assert_eq!(geometry.cell_col_containing_x(center_x), col);
        }
        for row in [0, 7, 19] {
            let center_z = geometry.cell_to_world_z(row) + GRID_CELL_SIZE / 2.0;
            assert_eq!(geometry.cell_row_containing_z(center_z), row);
        }
    }

    #[test]
    fn containing_lookup_floors_while_nearest_lookup_rounds() {
        let geometry = MapGeometry::new(10, 10);
        // Just past a grid line: still the same containing cell, but the
        // nearest grid line snaps back.
        let line_x = geometry.cell_to_world_x(4);
        assert_eq!(geometry.cell_col_containing_x(line_x + 0.1), 4);
        assert_eq!(geometry.nearest_grid_col_to_x(line_x + 0.1), 4);
        // Just before the next line: containing cell unchanged, nearest
        // line rounds up.
        let almost_next = line_x + GRID_CELL_SIZE - 0.1;
        assert_eq!(geometry.cell_col_containing_x(almost_next), 4);
        assert_eq!(geometry.nearest_grid_col_to_x(almost_next), 5);
    }

    #[test]
    fn map_is_centered_on_the_origin() {
        let geometry = MapGeometry::new(10, 6);
        assert_eq!(geometry.cell_to_world_x(0), -geometry.width() / 2.0);
        assert!((geometry.cell_to_world_x(10) - geometry.width() / 2.0).abs() < 1e-4);
        assert_eq!(geometry.cell_to_world_z(0), -geometry.depth() / 2.0);
        assert!((geometry.cell_to_world_z(6) - geometry.depth() / 2.0).abs() < 1e-4);
    }
}
