use bevy_math::{Vec3, Vec3Swizzles};

use crate::{
    config::MapGeometryConfig,
    constants::{BARRIER_THICKNESS_FRACTION, BRIDGE_THICKNESS_FRACTION},
    map::MapGeometry,
    math::PHYSICS_EPSILON,
    protocol::Ramp,
};

pub(crate) const CELL: f32 = 3.4;
pub(crate) const LEVEL_HEIGHT: f32 = 4.4;
pub(crate) const FLOOR_THICKNESS: f32 = 0.4;
pub(crate) const WALL_THICKNESS: f32 = 0.3;
pub(crate) const WALL_HEIGHT: f32 = LEVEL_HEIGHT - FLOOR_THICKNESS;
pub(crate) const BARRIER_THICKNESS: f32 = WALL_THICKNESS * BARRIER_THICKNESS_FRACTION;
pub(crate) const BRIDGE_THICKNESS: f32 = FLOOR_THICKNESS * BRIDGE_THICKNESS_FRACTION;

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

// A ramp's surface height over (x, z), clamped to the footprint along the run.
pub(crate) fn ramp_surface_at(ramp: &Ramp, x: f32, z: f32) -> f32 {
    let corners = ramp.corners();
    let run = (corners.high[0] - corners.low[0]).xz();
    let length_squared = run.length_squared();
    if length_squared < PHYSICS_EPSILON * PHYSICS_EPSILON {
        return ramp.y;
    }
    let progress = ((Vec3::new(x, 0.0, z) - corners.low[0]).xz().dot(run) / length_squared).clamp(0.0, 1.0);
    ramp.y + progress * ramp.height
}
