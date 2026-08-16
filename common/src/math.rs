use bevy_math::Vec3;

pub fn angle_delta_radians(a: f32, b: f32) -> f32 {
    (a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

// The one yaw/pitch → unit-direction convention shared by aim, projectile,
// and missile math. Client prediction and server simulation must agree on
// it, so every site routes through here.
#[must_use]
pub fn direction_from_yaw_pitch(yaw: f32, pitch: f32) -> Vec3 {
    let pitch_cos = pitch.cos();
    Vec3::new(yaw.sin() * pitch_cos, pitch.sin(), yaw.cos() * pitch_cos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_delta_wraps_at_pi() {
        assert!((angle_delta_radians(3.0, -3.0) - (6.0 - std::f32::consts::TAU)).abs() < 1e-6);
        assert!((angle_delta_radians(0.1, -0.1) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn direction_from_yaw_pitch_is_unit_and_matches_axes() {
        assert!((direction_from_yaw_pitch(0.0, 0.0) - Vec3::Z).length() < 1e-6);
        assert!((direction_from_yaw_pitch(std::f32::consts::FRAC_PI_2, 0.0) - Vec3::X).length() < 1e-6);
        assert!((direction_from_yaw_pitch(0.0, std::f32::consts::FRAC_PI_2) - Vec3::Y).length() < 1e-6);
        let arbitrary = direction_from_yaw_pitch(1.1, -0.6);
        assert!((arbitrary.length() - 1.0).abs() < 1e-6);
    }
}
