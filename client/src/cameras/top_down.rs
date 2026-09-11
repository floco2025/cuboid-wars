use bevy::prelude::*;

use common::{
    config::MapGeometryConfig,
    protocol::{MapLayout, Position},
};

// Fallback half-extent used before the server-supplied `MapLayout` arrives.
// Once it does, we use the actual floor footprint instead.
const FALLBACK_HALF_EXTENT: f32 = 40.0;

// The framed floor footprint on the XZ plane: `Rect` x is world x, y is world z.
fn fallback_bounds() -> Rect {
    Rect::from_center_half_size(Vec2::ZERO, Vec2::splat(FALLBACK_HALF_EXTENT))
}

pub(super) fn window_aspect_ratio(windows: &Query<&Window>) -> f32 {
    windows
        .single()
        .map_or(16.0 / 9.0, |window| window.width() / window.height().max(1.0))
}

pub(super) fn topdown_camera_transform(
    player_pos: &Position,
    map_layout: Option<&MapLayout>,
    geometry: MapGeometryConfig,
    aspect_ratio: f32,
    fov: f32,
    yaw: f32,
    margin: f32,
    tilt_degrees: f32,
) -> Transform {
    let view_direction = topdown_view_direction(yaw);
    let player_level = geometry.nearest_level_to_y(player_pos.y);
    let floor_bounds = map_layout.map_or_else(fallback_bounds, |layout| floor_bounds_for_level(layout, player_level));
    let center = floor_bounds.center();
    let mut target = Vec3::new(center.x, geometry.level_y(player_level), center.y);
    let camera_offset = topdown_camera_offset_to_fit(
        floor_bounds,
        aspect_ratio,
        fov,
        view_direction,
        margin,
        tilt_degrees,
        geometry.level_height,
    );
    let center_shift = projected_center_shift(floor_bounds, camera_offset, view_direction);
    target += view_direction * center_shift;

    Transform::from_translation(target + camera_offset).looking_at(target, Vec3::Y)
}

fn floor_bounds_for_level(map_layout: &MapLayout, level: u8) -> Rect {
    // Carried floors are in their carrier's frame and move; the view frames
    // the map itself.
    let bounds = map_layout
        .floors
        .iter()
        .filter(|floor| floor.carrier.is_world() && floor.level == level)
        .fold(Rect::EMPTY, |bounds, floor| {
            let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
            bounds.union(Rect::new(min_x, min_z, max_x, max_z))
        });
    if bounds.is_empty() { fallback_bounds() } else { bounds }
}

fn topdown_camera_offset_to_fit(
    bounds: Rect,
    aspect_ratio: f32,
    fov: f32,
    view_direction: Vec3,
    margin: f32,
    tilt_degrees: f32,
    min_distance: f32,
) -> Vec3 {
    let tilt = tilt_degrees.to_radians();
    let half_vertical_fov_tan = (fov / 2.0).tan();
    let half_horizontal_fov_tan = half_vertical_fov_tan * aspect_ratio.max(0.1);
    let view_extent = floor_extent_along_view(bounds, view_direction);
    let cross_extent = floor_extent_across_view(bounds, view_direction);
    let cross_distance = cross_extent * margin / (2.0 * half_horizontal_fov_tan);
    let view_distance = view_extent * tilt.cos() * margin / (2.0 * half_vertical_fov_tan);
    let view_distance = cross_distance.max(view_distance).max(min_distance);

    Vec3::Y * (view_distance * tilt.cos()) + view_direction * (view_distance * tilt.sin())
}

fn projected_center_shift(bounds: Rect, camera_offset: Vec3, view_direction: Vec3) -> f32 {
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

fn floor_extent_along_view(bounds: Rect, view_direction: Vec3) -> f32 {
    if view_direction.x.abs() > view_direction.z.abs() {
        bounds.width()
    } else {
        bounds.height()
    }
}

fn floor_extent_across_view(bounds: Rect, view_direction: Vec3) -> f32 {
    if view_direction.x.abs() > view_direction.z.abs() {
        bounds.height()
    } else {
        bounds.width()
    }
}

fn topdown_view_direction(yaw: f32) -> Vec3 {
    Vec3::new(yaw.sin(), 0.0, yaw.cos())
}

#[cfg(test)]
#[path = "tests/top_down.rs"]
mod tests;
