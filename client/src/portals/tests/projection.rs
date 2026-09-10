use super::*;

fn assert_vec3_close(actual: Vec3, expected: Vec3) {
    assert!(actual.distance(expected) < 1e-5, "{actual:?} != {expected:?}");
}

#[test]
fn camera_eye_maps_through_same_wall_pair() {
    let entry = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);
    let exit = PortalFrame::from_surface(Vec3::new(5.0, 0.0, 0.0), Vec3::Z, 0.0);
    let eye = entry.center + entry.right * 0.2 + entry.up * 0.3 + entry.normal * 3.0;
    let (transform, _) =
        portal_camera_view(eye, &entry, &exit, 100.0, full_aperture()).expect("eye in front of portal");
    let expected = exit.center - exit.right * 0.2 + exit.up * 0.3 - exit.normal * 3.0;
    assert_vec3_close(transform.translation, expected);
    assert_vec3_close(transform.forward().as_vec3(), exit.normal);
    assert_vec3_close(transform.up().as_vec3(), exit.up);
}

#[test]
fn projection_maps_aperture_corners_to_viewport_corners() {
    let entry = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);
    let exit = PortalFrame::from_surface(Vec3::new(5.0, 1.0, 0.0), Vec3::Z, 0.0);
    let eye = entry.center + entry.right * 0.25 + entry.up * 0.4 + entry.normal * 3.0;
    let (transform, projection) =
        portal_camera_view(eye, &entry, &exit, 100.0, full_aperture()).expect("eye in front of portal");
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
fn camera_view_rejects_eye_behind_aperture() {
    let entry = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);
    let exit = PortalFrame::from_surface(Vec3::X, Vec3::NEG_Z, 0.0);
    assert!(portal_camera_view(Vec3::NEG_Z, &entry, &exit, 100.0, full_aperture()).is_none());
}

#[test]
fn camera_view_stays_valid_with_the_eye_almost_on_the_aperture() {
    let entry = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);
    let exit = PortalFrame::from_surface(Vec3::X, Vec3::NEG_Z, 0.0);
    assert!(portal_camera_view(Vec3::Z * 0.01, &entry, &exit, 100.0, full_aperture()).is_some());
}

#[test]
fn projection_maps_a_sub_rectangle_of_the_aperture_to_the_viewport() {
    let entry = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);
    let exit = PortalFrame::from_surface(Vec3::new(5.0, 1.0, 0.0), Vec3::Z, 0.0);
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
