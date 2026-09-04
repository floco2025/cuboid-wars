// Reference sizes for tests that lay out a world by hand: the shipped maps'
// values, so hand-built fixtures and generated maps agree.
use common::{config::MapGeometryConfig, constants::BARRIER_THICKNESS_FRACTION, map::MapGeometry};

pub(crate) const CELL: f32 = 3.4;
pub(crate) const LEVEL_HEIGHT: f32 = 4.4;
pub(crate) const FLOOR_THICKNESS: f32 = 0.4;
pub(crate) const WALL_THICKNESS: f32 = 0.3;
pub(crate) const WALL_HEIGHT: f32 = LEVEL_HEIGHT - FLOOR_THICKNESS;
pub(crate) const BARRIER_THICKNESS: f32 = WALL_THICKNESS * BARRIER_THICKNESS_FRACTION;

pub(crate) fn sizes() -> MapGeometryConfig {
    MapGeometryConfig {
        grid_cell_size: CELL,
        level_height: LEVEL_HEIGHT,
        floor_thickness: FLOOR_THICKNESS,
        wall_thickness: WALL_THICKNESS,
    }
}

pub(crate) fn geometry(grid_cols: i32, grid_rows: i32) -> MapGeometry {
    MapGeometry::new(grid_cols, grid_rows, sizes())
}
