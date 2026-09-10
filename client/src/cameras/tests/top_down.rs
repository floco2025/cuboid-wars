use super::*;
use crate::test_fixtures::{LEVEL_HEIGHT, sizes};
use common::protocol::CarrierId;
use std::f32::consts::FRAC_PI_2;

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
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    }
}

#[test]
fn view_direction_follows_yaw() {
    assert!((topdown_view_direction(0.0) - Vec3::Z).length() < 1e-6);
    assert!((topdown_view_direction(FRAC_PI_2) - Vec3::X).length() < 1e-6);
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
    let transform = topdown_camera_transform(&player, Some(&layout), sizes(), 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);

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
    let wide = topdown_camera_transform(&player, Some(&layout), sizes(), 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);
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
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let close = topdown_camera_transform(&player, Some(&tiny), sizes(), 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);
    assert!(close.translation.y >= LEVEL_HEIGHT - 1e-3);
}

#[test]
fn missing_layout_and_empty_level_fall_back_to_default_bounds() {
    let player = Position { x: 3.0, y: 0.0, z: 4.0 };
    let fallback = topdown_camera_transform(&player, None, sizes(), 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);

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
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let empty_level = topdown_camera_transform(&player, Some(&elevated), sizes(), 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);
    assert!((fallback.translation - empty_level.translation).length() < 1e-3);
}

#[test]
fn tilt_pushes_the_camera_back_along_the_view_direction() {
    let layout = wide_layout();
    let player = Position::default();
    let straight = topdown_camera_transform(&player, Some(&layout), sizes(), 16.0 / 9.0, 1.0, 0.0, 1.1, 0.0);
    let tilted = topdown_camera_transform(&player, Some(&layout), sizes(), 16.0 / 9.0, 1.0, 0.0, 1.1, 30.0);
    assert!(
        tilted.translation.z.abs() > straight.translation.z.abs() + 1.0,
        "a tilted camera stands off horizontally"
    );
    assert!(
        tilted.translation.y < straight.translation.y,
        "tilt trades height for standoff"
    );
}
