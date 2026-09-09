use std::f32::consts::{PI, TAU};

use crate::constants::PLAYER_MOVEMENT_TRUST_DISTANCE;
use bevy_math::{Quat, Vec3};
use rapier3d::prelude::{Pose, Vector, glamx};

pub const PHYSICS_EPSILON: f32 = 1e-6;

// Rapier's glam is not Bevy's; every value crossing between the two converts here.
#[must_use]
pub fn to_rapier(v: Vec3) -> Vector {
    Vector::from_array(v.to_array())
}

#[must_use]
pub fn from_rapier(v: Vector) -> Vec3 {
    Vec3::from_array(v.to_array())
}

#[must_use]
pub fn rapier_pose(translation: Vec3, rotation: Quat) -> Pose {
    Pose::from_parts(to_rapier(translation), glamx::Quat::from_array(rotation.to_array()))
}

pub fn angle_delta_radians(a: f32, b: f32) -> f32 {
    (a - b + PI).rem_euclid(TAU) - PI
}

// The one yaw/pitch → unit-direction convention shared by aim, projectile,
// and missile math. Client prediction and server simulation must agree on
// it, so every site routes through here.
#[must_use]
pub fn direction_from_yaw_pitch(yaw: f32, pitch: f32) -> Vec3 {
    let pitch_cos = pitch.cos();
    Vec3::new(yaw.sin() * pitch_cos, pitch.sin(), yaw.cos() * pitch_cos)
}

// Pick the axis with the largest |value| from a 3-component delta. Used
// for the per-axis snap decision and its warning log, so the reader sees
// which axis tripped the threshold.
pub fn worst_axis_divergence(delta: Vec3) -> (&'static str, f32) {
    let xa = delta.x.abs();
    let ya = delta.y.abs();
    let za = delta.z.abs();
    if xa >= ya && xa >= za {
        ("x", xa)
    } else if ya >= za {
        ("y", ya)
    } else {
        ("z", za)
    }
}

#[must_use]
pub fn player_movement_is_trusted(delta: Vec3) -> bool {
    delta.is_finite() && worst_axis_divergence(delta).1 < PLAYER_MOVEMENT_TRUST_DISTANCE
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn movement_trust_is_finite_and_strictly_per_axis() {
        assert!(player_movement_is_trusted(Vec3::splat(4.99)));
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            for sign in [-1.0, 1.0] {
                assert!(player_movement_is_trusted(axis * sign * 4.99));
                assert!(!player_movement_is_trusted(axis * sign * 5.0));
                assert!(!player_movement_is_trusted(axis * sign * 5.01));
            }
        }
        assert!(!player_movement_is_trusted(Vec3::splat(f32::NAN)));
        assert!(!player_movement_is_trusted(Vec3::splat(f32::INFINITY)));
    }

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
    #[test]
    fn divergence_picks_the_dominant_axis() {
        assert_eq!(worst_axis_divergence(Vec3::new(-3.0, 1.0, 2.0)), ("x", 3.0));
        assert_eq!(worst_axis_divergence(Vec3::new(1.0, -3.0, 2.0)), ("y", 3.0));
        assert_eq!(worst_axis_divergence(Vec3::new(1.0, 2.0, -3.0)), ("z", 3.0));
    }

    #[test]
    fn divergence_ties_prefer_x_then_y() {
        assert_eq!(worst_axis_divergence(Vec3::splat(2.0)), ("x", 2.0));
        assert_eq!(worst_axis_divergence(Vec3::new(0.0, 2.0, 2.0)), ("y", 2.0));
    }
}
