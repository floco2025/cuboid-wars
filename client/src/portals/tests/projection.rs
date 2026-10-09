use super::*;
use crate::test_fixtures::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, gameplay_config, portal_frame};

fn assert_vec3_close(actual: Vec3, expected: Vec3) {
    assert!(actual.distance(expected) < 1e-5, "{actual:?} != {expected:?}");
}

#[test]
fn camera_eye_maps_through_same_wall_pair() {
    let entry = portal_frame(Vec3::ZERO, Vec3::Z);
    let exit = portal_frame(Vec3::new(5.0, 0.0, 0.0), Vec3::Z);
    let eye = entry.center + entry.right * 0.2 + entry.up * 0.3 + entry.normal * 3.0;
    let (transform, _) = portal_camera_view(eye, &entry, &exit, 100.0, full_aperture(gameplay_config().portals.size))
        .expect("eye in front of portal");
    let expected = exit.center - exit.right * 0.2 + exit.up * 0.3 - exit.normal * 3.0;
    assert_vec3_close(transform.translation, expected);
    assert_vec3_close(transform.forward().as_vec3(), exit.normal);
    assert_vec3_close(transform.up().as_vec3(), exit.up);
}

#[test]
fn projection_maps_aperture_corners_to_viewport_corners() {
    let entry = portal_frame(Vec3::ZERO, Vec3::Z);
    let exit = portal_frame(Vec3::new(5.0, 1.0, 0.0), Vec3::Z);
    let eye = entry.center + entry.right * 0.25 + entry.up * 0.4 + entry.normal * 3.0;
    let (transform, projection) =
        portal_camera_view(eye, &entry, &exit, 100.0, full_aperture(gameplay_config().portals.size))
            .expect("eye in front of portal");
    let view_from_world = transform.to_matrix().inverse();

    for (point, expected) in [
        (
            exit.center + exit.right * PORTAL_HALF_WIDTH - exit.up * PORTAL_HALF_HEIGHT,
            Vec2::new(-1.0, -1.0),
        ),
        (
            exit.center - exit.right * PORTAL_HALF_WIDTH + exit.up * PORTAL_HALF_HEIGHT,
            Vec2::new(1.0, 1.0),
        ),
    ] {
        let clip = projection.matrix() * view_from_world * point.extend(1.0);
        let ndc = clip.truncate() / clip.w;
        assert!((ndc.x - expected.x).abs() < 1e-4, "{ndc:?}");
        assert!((ndc.y - expected.y).abs() < 1e-4, "{ndc:?}");
    }
}

#[test]
fn camera_view_needs_the_eye_in_front_of_the_aperture_however_close() {
    let entry = portal_frame(Vec3::ZERO, Vec3::Z);
    let exit = portal_frame(Vec3::X, Vec3::NEG_Z);
    let view = |eye| portal_camera_view(eye, &entry, &exit, 100.0, full_aperture(gameplay_config().portals.size));
    assert!(view(Vec3::NEG_Z).is_none());
    assert!(view(Vec3::Z * 0.01).is_some());
}

#[test]
fn projection_maps_a_sub_rectangle_of_the_aperture_to_the_viewport() {
    let entry = portal_frame(Vec3::ZERO, Vec3::Z);
    let exit = portal_frame(Vec3::new(5.0, 1.0, 0.0), Vec3::Z);
    let eye = entry.normal * 0.4;
    let rect = Rect::new(0.0, 0.0, PORTAL_HALF_WIDTH, PORTAL_HALF_HEIGHT);
    let (transform, projection) = portal_camera_view(eye, &entry, &exit, 100.0, rect).expect("eye in front of portal");
    let view_from_world = transform.to_matrix().inverse();

    // Entry-local +right maps to exit -right; the rect's corners land on the viewport's.
    for (point, expected) in [
        (exit.center, Vec2::new(-1.0, -1.0)),
        (
            exit.center - exit.right * PORTAL_HALF_WIDTH + exit.up * PORTAL_HALF_HEIGHT,
            Vec2::new(1.0, 1.0),
        ),
    ] {
        let clip = projection.matrix() * view_from_world * point.extend(1.0);
        let ndc = clip.truncate() / clip.w;
        assert!((ndc.x - expected.x).abs() < 1e-4, "{ndc:?}");
        assert!((ndc.y - expected.y).abs() < 1e-4, "{ndc:?}");
    }
}
