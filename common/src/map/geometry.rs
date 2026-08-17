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
