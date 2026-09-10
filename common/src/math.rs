use std::f32::consts::{PI, TAU};

use bevy_math::{Quat, Vec3};
use rapier3d::prelude::{Pose, Vector, glamx};

pub const PHYSICS_EPSILON: f32 = 1e-6;

// Wire sequence numbers wrap; `seq` is newer than `last` when it is ahead by
// less than half the range.
#[must_use]
pub const fn sequence_is_newer(seq: u32, last: u32) -> bool {
    seq != last && seq.wrapping_sub(last) < (1 << 31)
}

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

#[cfg(test)]
#[path = "tests/math.rs"]
mod tests;
