use super::steering::sweep_clear;
use bevy::prelude::*;
use common::{physics::CollisionWorld, protocol::BarrierId};
use rand::RngExt;
use std::f32::consts::TAU;
const LAUNCH_SAMPLES: usize = 8;

// Resample the random spread until the launch has a clear runway; a boxed-in
// muzzle falls back to the straight aim used to acquire the lock.
#[expect(clippy::too_many_arguments, reason = "launch sweep needs the full collision context")]
pub fn clear_launch_direction(
    aim: Vec3,
    spread_rad: f32,
    muzzle: Vec3,
    runway: f32,
    radius: f32,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierId],
    rng: &mut impl RngExt,
) -> Vec3 {
    for _ in 0..LAUNCH_SAMPLES {
        let candidate = launch_direction(aim, spread_rad, rng);
        if sweep_clear(collision_world, open_kinds, muzzle, candidate * runway, radius) {
            return candidate;
        }
    }
    aim
}

// Random direction within the spread cone around `aim`: uniform azimuth,
// tilt sampled in [spread/2, spread] — a minimum deviation so the corrective
// curve is always visible. Zero spread launches straight.
fn launch_direction(aim: Vec3, spread_rad: f32, rng: &mut impl RngExt) -> Vec3 {
    if spread_rad <= 0.0 {
        return aim;
    }
    let tilt = rng.random_range(spread_rad / 2.0..=spread_rad);
    let azimuth = rng.random_range(0.0..TAU);
    let tilt_axis = Quat::from_axis_angle(aim, azimuth) * aim.any_orthonormal_vector();
    Quat::from_axis_angle(tilt_axis, tilt) * aim
}

#[cfg(test)]
#[path = "tests/launch.rs"]
mod tests;
