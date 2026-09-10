use super::*;
use common::math::angle_delta_radians;

#[test]
fn view_through_a_facing_pair_is_preserved_without_tilt() {
    let entry = PortalFrame::from_surface(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, 0.0);
    let exit = PortalFrame::from_surface(Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z, 0.0);
    let (seeded, yaw, pitch) = portal_view_transition(&entry, &exit, 0.0, -0.3, 0.0);
    assert!(yaw.abs() < 1e-4);
    assert!((pitch + 0.3).abs() < 1e-4);
    let target = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    assert!(seeded.angle_between(target) < 1e-3);
}

#[test]
fn view_through_a_same_wall_pair_turns_around_without_tilt() {
    let entry = PortalFrame::from_surface(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, 0.0);
    let exit = PortalFrame::from_surface(Vec3::new(5.0, 1.0, 0.0), Vec3::Z, 0.0);
    let (seeded, yaw, pitch) = portal_view_transition(&entry, &exit, 0.0, 0.2, 0.0);
    assert!(angle_delta_radians(yaw, PI).abs() < 1e-4);
    assert!((pitch - 0.2).abs() < 1e-4);
    let target = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    assert!(seeded.angle_between(target) < 1e-3);
}
