use bevy_ecs::prelude::Resource;

use crate::config::MapGeometryConfig;

// Grid ↔ world conversion for one map: the grid is centered on the world
// origin, and `sizes` carries the map's cell size and storey height.
#[derive(Debug, Clone, Copy, Resource)]
pub struct MapGeometry {
    pub grid_cols: i32,
    pub grid_rows: i32,
    pub sizes: MapGeometryConfig,
}

impl MapGeometry {
    #[must_use]
    pub const fn new(grid_cols: i32, grid_rows: i32, sizes: MapGeometryConfig) -> Self {
        Self {
            grid_cols,
            grid_rows,
            sizes,
        }
    }

    #[must_use]
    pub fn cell_size(&self) -> f32 {
        self.sizes.grid_cell_size
    }

    #[must_use]
    pub fn wall_height(&self) -> f32 {
        self.sizes.wall_height()
    }

    #[must_use]
    pub fn level_height(&self) -> f32 {
        self.sizes.level_height
    }

    #[must_use]
    pub fn floor_thickness(&self) -> f32 {
        self.sizes.floor_thickness
    }

    #[must_use]
    pub fn wall_thickness(&self) -> f32 {
        self.sizes.wall_thickness
    }

    #[must_use]
    pub fn wall_half_thickness(&self) -> f32 {
        self.sizes.wall_half_thickness()
    }

    #[must_use]
    pub fn barrier_thickness(&self) -> f32 {
        self.sizes.barrier_thickness()
    }

    #[must_use]
    pub fn bridge_thickness(&self) -> f32 {
        self.sizes.bridge_thickness()
    }

    #[must_use]
    pub fn level_y(&self, level: u8) -> f32 {
        self.sizes.level_y(level)
    }

    #[must_use]
    pub fn level_for_y(&self, y: f32) -> u8 {
        self.sizes.level_for_y(y)
    }

    #[must_use]
    pub fn width(&self) -> f32 {
        self.grid_cols as f32 * self.cell_size()
    }

    #[must_use]
    pub fn depth(&self) -> f32 {
        self.grid_rows as f32 * self.cell_size()
    }

    // The west edge of column `col`.
    #[must_use]
    pub fn cell_to_world_x(&self, col: i32) -> f32 {
        (col as f32).mul_add(self.cell_size(), -(self.width() / 2.0))
    }

    // The north edge of row `row`.
    #[must_use]
    pub fn cell_to_world_z(&self, row: i32) -> f32 {
        (row as f32).mul_add(self.cell_size(), -(self.depth() / 2.0))
    }

    #[must_use]
    pub fn cell_center_x(&self, col: i32) -> f32 {
        self.cell_to_world_x(col) + self.cell_size() / 2.0
    }

    #[must_use]
    pub fn cell_center_z(&self, row: i32) -> f32 {
        self.cell_to_world_z(row) + self.cell_size() / 2.0
    }

    #[must_use]
    pub fn cell_col_containing_x(&self, x: f32) -> i32 {
        ((x + self.width() / 2.0) / self.cell_size()).floor() as i32
    }

    #[must_use]
    pub fn cell_row_containing_z(&self, z: f32) -> i32 {
        ((z + self.depth() / 2.0) / self.cell_size()).floor() as i32
    }

    #[must_use]
    pub fn nearest_grid_col_to_x(&self, x: f32) -> i32 {
        ((x + self.width() / 2.0) / self.cell_size()).round() as i32
    }

    #[must_use]
    pub fn nearest_grid_row_to_z(&self, z: f32) -> i32 {
        ((z + self.depth() / 2.0) / self.cell_size()).round() as i32
    }
}

#[cfg(test)]
mod tests {

    use crate::test_geometry::{CELL as GRID_CELL_SIZE, geometry};

    #[test]
    fn cell_center_round_trips_to_the_same_cell() {
        let geometry = geometry(30, 20);
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
        let geometry = geometry(10, 10);
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
        let geometry = geometry(10, 6);
        assert_eq!(geometry.cell_to_world_x(0), -geometry.width() / 2.0);
        assert!((geometry.cell_to_world_x(10) - geometry.width() / 2.0).abs() < 1e-4);
        assert_eq!(geometry.cell_to_world_z(0), -geometry.depth() / 2.0);
        assert!((geometry.cell_to_world_z(6) - geometry.depth() / 2.0).abs() < 1e-4);
    }
}
