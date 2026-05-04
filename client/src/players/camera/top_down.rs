use bevy::prelude::*;

use crate::{constants::*, map::visual_focus_level};
use common::{
    constants::{LEVEL_HEIGHT, MAP_DEPTH, MAP_WIDTH},
    protocol::{Floor, MapLayout, Position},
};

#[derive(Copy, Clone)]
struct FloorBounds {
    min_x: f32,
    max_x: f32,
    min_z: f32,
    max_z: f32,
}

impl FloorBounds {
    const fn fallback() -> Self {
        Self {
            min_x: -MAP_WIDTH / 2.0,
            max_x: MAP_WIDTH / 2.0,
            min_z: -MAP_DEPTH / 2.0,
            max_z: MAP_DEPTH / 2.0,
        }
    }

    fn include_floor(&mut self, floor: &Floor) {
        let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
        self.min_x = self.min_x.min(min_x);
        self.max_x = self.max_x.max(max_x);
        self.min_z = self.min_z.min(min_z);
        self.max_z = self.max_z.max(max_z);
    }

    fn center(self) -> Vec3 {
        Vec3::new(
            f32::midpoint(self.min_x, self.max_x),
            0.0,
            f32::midpoint(self.min_z, self.max_z),
        )
    }

    fn width(self) -> f32 {
        self.max_x - self.min_x
    }

    fn depth(self) -> f32 {
        self.max_z - self.min_z
    }
}

pub(super) fn window_aspect_ratio(windows: &Query<&Window>) -> f32 {
    windows
        .single()
        .map_or(16.0 / 9.0, |window| window.width() / window.height().max(1.0))
}

pub(super) fn topdown_camera_transform(
    player_pos: &Position,
    map_layout: Option<&MapLayout>,
    aspect_ratio: f32,
    fov: f32,
    yaw: f32,
) -> Transform {
    let view_direction = topdown_view_direction(yaw);
    let player_level = visual_focus_level(player_pos.y);
    let floor_bounds = map_layout.map_or_else(FloorBounds::fallback, |layout| {
        floor_bounds_for_level(layout, player_level)
    });
    let mut target = floor_bounds.center();
    target.y = f32::from(player_level) * LEVEL_HEIGHT;
    let camera_offset = topdown_camera_offset_to_fit(floor_bounds, aspect_ratio, fov, view_direction);
    let center_shift = projected_center_shift(floor_bounds, camera_offset, view_direction);
    target += view_direction * center_shift;

    Transform::from_translation(target + camera_offset).looking_at(target, Vec3::Y)
}

fn floor_bounds_for_level(map_layout: &MapLayout, level: u8) -> FloorBounds {
    let mut bounds = FloorBounds {
        min_x: f32::INFINITY,
        max_x: f32::NEG_INFINITY,
        min_z: f32::INFINITY,
        max_z: f32::NEG_INFINITY,
    };

    for floor in map_layout.floors.iter().filter(|floor| floor.level == level) {
        bounds.include_floor(floor);
    }

    if bounds.min_x.is_finite() {
        bounds
    } else {
        FloorBounds::fallback()
    }
}

fn topdown_camera_offset_to_fit(bounds: FloorBounds, aspect_ratio: f32, fov: f32, view_direction: Vec3) -> Vec3 {
    let tilt = TOPDOWN_TILT_DEGREES.to_radians();
    let half_vertical_fov_tan = (fov / 2.0).tan();
    let half_horizontal_fov_tan = half_vertical_fov_tan * aspect_ratio.max(0.1);
    let view_extent = floor_extent_along_view(bounds, view_direction);
    let cross_extent = floor_extent_across_view(bounds, view_direction);
    let cross_distance = cross_extent * TOPDOWN_MARGIN / (2.0 * half_horizontal_fov_tan);
    let view_distance = view_extent * tilt.cos() * TOPDOWN_MARGIN / (2.0 * half_vertical_fov_tan);
    let view_distance = cross_distance.max(view_distance).max(LEVEL_HEIGHT);

    Vec3::Y * (view_distance * tilt.cos()) + view_direction * (view_distance * tilt.sin())
}

fn projected_center_shift(bounds: FloorBounds, camera_offset: Vec3, view_direction: Vec3) -> f32 {
    let half_view_extent = floor_extent_along_view(bounds, view_direction) / 2.0;
    let view_offset = camera_offset.dot(view_direction);
    if half_view_extent <= 0.0 || view_offset.abs() <= f32::EPSILON {
        return 0.0;
    }

    let distance_squared = camera_offset.length_squared();
    let discriminant = distance_squared.mul_add(
        distance_squared,
        4.0 * view_offset * view_offset * half_view_extent * half_view_extent,
    );
    ((discriminant.sqrt() - distance_squared) / (2.0 * view_offset)).clamp(-half_view_extent, half_view_extent)
}

fn floor_extent_along_view(bounds: FloorBounds, view_direction: Vec3) -> f32 {
    if view_direction.x.abs() > view_direction.z.abs() {
        bounds.width()
    } else {
        bounds.depth()
    }
}

fn floor_extent_across_view(bounds: FloorBounds, view_direction: Vec3) -> f32 {
    if view_direction.x.abs() > view_direction.z.abs() {
        bounds.depth()
    } else {
        bounds.width()
    }
}

fn topdown_view_direction(yaw: f32) -> Vec3 {
    Vec3::new(yaw.sin(), 0.0, yaw.cos())
}
