use bevy_ecs::prelude::Resource;

use crate::config::MapGeometryConfig;

// Grid coordinates resolve in the carrier's frame, centered on its origin (world space on the world carrier).
#[derive(Debug, Clone, Copy, Resource)]
pub struct MapGeometry {
    pub grid_cols: i32,
    pub grid_rows: i32,
    sizes: MapGeometryConfig,
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
    pub fn wall_light_height(&self) -> f32 {
        self.sizes.wall_light_height()
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
    pub fn nearest_level_to_y(&self, y: f32) -> u8 {
        self.sizes.nearest_level_to_y(y)
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
#[path = "tests/geometry.rs"]
mod tests;
