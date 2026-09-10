use super::*;
use std::f32::consts::FRAC_PI_2;

#[test]
fn angle_delta_wraps_at_pi() {
    assert!((angle_delta_radians(3.0, -3.0) - (6.0 - TAU)).abs() < 1e-6);
    assert!((angle_delta_radians(0.1, -0.1) - 0.2).abs() < 1e-6);
}

#[test]
fn direction_from_yaw_pitch_is_unit_and_matches_axes() {
    assert!((direction_from_yaw_pitch(0.0, 0.0) - Vec3::Z).length() < 1e-6);
    assert!((direction_from_yaw_pitch(FRAC_PI_2, 0.0) - Vec3::X).length() < 1e-6);
    assert!((direction_from_yaw_pitch(0.0, FRAC_PI_2) - Vec3::Y).length() < 1e-6);
    let arbitrary = direction_from_yaw_pitch(1.1, -0.6);
    assert!((arbitrary.length() - 1.0).abs() < 1e-6);
}
