use super::*;
use crate::test_fixtures::follow_camera;

#[test]
fn obstruction_does_not_reduce_requested_distance_when_scrolling_out() {
    let config = follow_camera();
    let mut camera = FollowCamera {
        distance: 4.0,
        arm_distance: 1.5,
        previous_pivot: Some(Vec3::ZERO),
        ..Default::default()
    };
    camera.zoom(
        CameraViewMode::ThirdPerson,
        -1.0,
        0.2 / INPUT_ZOOM_SENSITIVITY_BASE,
        config,
    );
    assert!((camera.distance - 4.2).abs() < 1e-5);
}

#[test]
fn zoom_into_first_person_locks_and_zooming_out_keeps_lock() {
    let config = follow_camera();
    let mut camera = FollowCamera {
        distance: 1.0,
        locked: false,
        ..Default::default()
    };
    camera.zoom(
        CameraViewMode::ThirdPerson,
        2.0,
        0.5 / INPUT_ZOOM_SENSITIVITY_BASE,
        config,
    );
    assert_eq!(camera.distance, 0.0);
    assert!(camera.locked);
    camera.zoom(
        CameraViewMode::FirstPerson,
        -2.0,
        0.5 / INPUT_ZOOM_SENSITIVITY_BASE,
        config,
    );
    assert!(camera.distance > config.first_person_distance);
    assert!(camera.locked);
}
#[test]
fn tiny_scroll_events_accumulate_without_flipping_modes() {
    let config = follow_camera();
    let mut camera = FollowCamera::default();
    for _ in 0..5 {
        camera.zoom(
            CameraViewMode::FirstPerson,
            -0.25,
            0.5 / INPUT_ZOOM_SENSITIVITY_BASE,
            config,
        );
        assert!(camera.distance <= config.first_person_distance);
    }
    camera.zoom(
        CameraViewMode::FirstPerson,
        -0.25,
        0.5 / INPUT_ZOOM_SENSITIVITY_BASE,
        config,
    );
    assert!(camera.distance > config.first_person_distance);
    camera.zoom(
        CameraViewMode::ThirdPerson,
        0.02,
        0.5 / INPUT_ZOOM_SENSITIVITY_BASE,
        config,
    );
    assert!(camera.distance > config.first_person_distance);
}
#[test]
fn top_down_restores_zoom_and_ignores_scroll() {
    let config = follow_camera();
    for (view, distance) in [(CameraViewMode::FirstPerson, 0.0), (CameraViewMode::ThirdPerson, 3.0)] {
        let mut camera = FollowCamera {
            distance,
            ..Default::default()
        };
        assert_eq!(camera.toggle_top_down(view), CameraViewMode::TopDown);
        camera.zoom(CameraViewMode::TopDown, 10.0, 0.5 / INPUT_ZOOM_SENSITIVITY_BASE, config);
        assert_eq!(camera.distance, distance);
        assert_eq!(camera.toggle_top_down(CameraViewMode::TopDown), view);
    }
}
#[test]
fn shoulder_offset_eases_in_with_the_pivot() {
    let config = FollowCameraConfig {
        shoulder_offset: 0.5,
        ..follow_camera()
    };
    let eased = FollowCamera {
        distance: 0.5,
        ..Default::default()
    };
    assert_eq!(eased.shoulder_offset(config), 0.25);
    let full = FollowCamera {
        distance: 3.0,
        ..Default::default()
    };
    assert_eq!(full.shoulder_offset(config), 0.5);
}
#[test]
fn sensitivity_scales_zoom_and_distance_is_bounded() {
    let config = follow_camera();
    let mut camera = FollowCamera {
        distance: 3.0,
        ..Default::default()
    };
    camera.zoom(
        CameraViewMode::ThirdPerson,
        -1.0,
        1.0 / INPUT_ZOOM_SENSITIVITY_BASE,
        config,
    );
    assert_eq!(camera.distance, 4.0);
    camera.zoom(
        CameraViewMode::ThirdPerson,
        -100.0,
        1.0 / INPUT_ZOOM_SENSITIVITY_BASE,
        config,
    );
    assert_eq!(camera.distance, config.max_distance);
    camera.zoom(
        CameraViewMode::ThirdPerson,
        100.0,
        1.0 / INPUT_ZOOM_SENSITIVITY_BASE,
        config,
    );
    assert_eq!(camera.distance, 0.0);
}
