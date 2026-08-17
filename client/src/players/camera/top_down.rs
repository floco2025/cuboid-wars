use bevy::prelude::*;

use crate::map::visual_focus_level;
use common::{
    constants::LEVEL_HEIGHT,
    protocol::{Floor, MapLayout, Position},
};

// Fallback half-extent used before the server-supplied `MapLayout` arrives.
// Once it does, we use the actual floor footprint instead.
const FALLBACK_HALF_EXTENT: f32 = 40.0;

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
            min_x: -FALLBACK_HALF_EXTENT,
            max_x: FALLBACK_HALF_EXTENT,
            min_z: -FALLBACK_HALF_EXTENT,
            max_z: FALLBACK_HALF_EXTENT,
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
    margin: f32,
    tilt_degrees: f32,
) -> Transform {
    let view_direction = topdown_view_direction(yaw);
    let player_level = visual_focus_level(player_pos.y);
    let floor_bounds = map_layout.map_or_else(FloorBounds::fallback, |layout| {
        floor_bounds_for_level(layout, player_level)
    });
    let mut target = floor_bounds.center();
    target.y = f32::from(player_level) * LEVEL_HEIGHT;
    let camera_offset =
        topdown_camera_offset_to_fit(floor_bounds, aspect_ratio, fov, view_direction, margin, tilt_degrees);
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

fn topdown_camera_offset_to_fit(
    bounds: FloorBounds,
    aspect_ratio: f32,
    fov: f32,
    view_direction: Vec3,
    margin: f32,
    tilt_degrees: f32,
) -> Vec3 {
    let tilt = tilt_degrees.to_radians();
    let half_vertical_fov_tan = (fov / 2.0).tan();
    let half_horizontal_fov_tan = half_vertical_fov_tan * aspect_ratio.max(0.1);
    let view_extent = floor_extent_along_view(bounds, view_direction);
    let cross_extent = floor_extent_across_view(bounds, view_direction);
    let cross_distance = cross_extent * margin / (2.0 * half_horizontal_fov_tan);
    let view_distance = view_extent * tilt.cos() * margin / (2.0 * half_vertical_fov_tan);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn wide_layout() -> MapLayout {
        MapLayout {
            floors: vec![Floor {
                x1: -20.0,
                z1: -10.0,
                x2: 20.0,
                z2: 10.0,
                y: 0.0,
                thickness: 1.0,
                level: 0,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn view_direction_follows_yaw() {
        assert!((topdown_view_direction(0.0) - Vec3::Z).length() < 1e-6);
        assert!((topdown_view_direction(std::f32::consts::FRAC_PI_2) - Vec3::X).length() < 1e-6);
    }

    #[test]
    fn extents_swap_with_the_view_axis() {
        let bounds = FloorBounds {
            min_x: -20.0,
            max_x: 20.0,
            min_z: -10.0,
            max_z: 10.0,
        };
        // Looking along Z: the along-view extent is the map depth.
        assert_eq!(floor_extent_along_view(bounds, Vec3::Z), 20.0);
        assert_eq!(floor_extent_across_view(bounds, Vec3::Z), 40.0);
        // Looking along X they swap.
        assert_eq!(floor_extent_along_view(bounds, Vec3::X), 40.0);
        assert_eq!(floor_extent_across_view(bounds, Vec3::X), 20.0);
    }

    #[test]
    fn camera_looks_at_the_floor_center_of_the_player_level() {
        let layout = wide_layout();
        let player = Position {
            x: 5.0,
            y: 0.0,
            z: -3.0,
        };
        let transform = topdown_camera_transform(&player, Some(&layout), 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);

        // Straight-down view (no tilt, yaw 0): the camera hangs over the
        // floor center, not over the player.
        assert!((transform.translation.x - 0.0).abs() < 1e-3);
        assert!((transform.translation.z - 0.0).abs() < 1e-3);
        assert!(transform.translation.y > 0.0, "camera must be above the map");
    }

    #[test]
    fn camera_height_covers_the_wider_axis_and_never_undershoots() {
        let layout = wide_layout();
        let player = Position::default();
        let wide = topdown_camera_transform(&player, Some(&layout), 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);
        // 40 m of width across a ~1 rad FOV needs far more height than the
        // LEVEL_HEIGHT floor.
        assert!(wide.translation.y > LEVEL_HEIGHT);

        // A tiny map still keeps the minimum height.
        let tiny = MapLayout {
            floors: vec![Floor {
                x1: -0.5,
                z1: -0.5,
                x2: 0.5,
                z2: 0.5,
                y: 0.0,
                thickness: 1.0,
                level: 0,
            }],
            ..Default::default()
        };
        let close = topdown_camera_transform(&player, Some(&tiny), 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);
        assert!(close.translation.y >= LEVEL_HEIGHT - 1e-3);
    }

    #[test]
    fn missing_layout_and_empty_level_fall_back_to_default_bounds() {
        let player = Position { x: 3.0, y: 0.0, z: 4.0 };
        let fallback = topdown_camera_transform(&player, None, 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);

        // A layout whose floors are all on another level behaves the same.
        let elevated = MapLayout {
            floors: vec![Floor {
                x1: -5.0,
                z1: -5.0,
                x2: 5.0,
                z2: 5.0,
                y: LEVEL_HEIGHT,
                thickness: 1.0,
                level: 1,
            }],
            ..Default::default()
        };
        let empty_level = topdown_camera_transform(&player, Some(&elevated), 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);
        assert!((fallback.translation - empty_level.translation).length() < 1e-3);
    }

    #[test]
    fn tilt_pushes_the_camera_back_along_the_view_direction() {
        let layout = wide_layout();
        let player = Position::default();
        let straight = topdown_camera_transform(&player, Some(&layout), 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);
        let tilted = topdown_camera_transform(&player, Some(&layout), 16.0 / 9.0, 1.0, 0.0, 1.1, 30.0);
        assert!(
            tilted.translation.z.abs() > straight.translation.z.abs() + 1.0,
            "a tilted camera stands off horizontally"
        );
        assert!(
            tilted.translation.y < straight.translation.y,
            "tilt trades height for standoff"
        );
    }
}
