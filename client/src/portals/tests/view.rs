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

#[test]
fn floor_to_floor_exit_keeps_the_pitch_and_turns_with_the_held_input() {
    let entry = PortalFrame::from_surface(Vec3::ZERO, Vec3::Y, 0.0);
    let exit = PortalFrame::from_surface(Vec3::new(10.0, 0.0, 0.0), Vec3::Y, 0.0);
    let (seeded, yaw, pitch) = portal_view_transition(&entry, &exit, 0.3, -1.2, 0.0);
    assert!((pitch + 1.2).abs() < 1e-4);
    let forward = |yaw: f32| Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
    let mapped = traverse_vector(&entry, &exit, forward(0.3));
    assert!((forward(yaw) - mapped).length() < 1e-4);
    let target = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    assert!(seeded.angle_between(target) < 1e-3);
}

#[test]
fn wall_to_floor_exit_keeps_the_horizon_and_faces_along_the_exit_up() {
    let entry = PortalFrame::from_surface(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, 0.0);
    let exit = PortalFrame::from_surface(Vec3::new(5.0, 0.0, 5.0), Vec3::Y, 0.0);
    let (seeded, yaw, pitch) = portal_view_transition(&entry, &exit, 0.0, -0.2, 0.0);
    assert!((pitch + 0.2).abs() < 1e-4);
    let forward = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
    assert!((forward - exit.up).length() < 1e-4);
    let target = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    assert!(seeded.angle_between(target) < 1e-3);
}
