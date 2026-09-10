use crate::{math::PHYSICS_EPSILON, protocol::Ramp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RampAxis {
    X,
    Z,
}

#[must_use]
pub fn ramp_axis(ramp: &Ramp) -> RampAxis {
    if (ramp.x2 - ramp.x1).abs() >= (ramp.z2 - ramp.z1).abs() {
        RampAxis::X
    } else {
        RampAxis::Z
    }
}

// Compute the surface Y of a single ramp at (x, z). Caller must have already
// verified that (x, z) lies inside `ramp.bounds_xz()`.
#[must_use]
pub fn ramp_surface_at(ramp: &Ramp, x: f32, z: f32) -> f32 {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();

    let progress = match ramp_axis(ramp) {
        RampAxis::X if (max_x - min_x).abs() >= PHYSICS_EPSILON => {
            ((x - ramp.x1) / (ramp.x2 - ramp.x1)).clamp(0.0, 1.0)
        }
        RampAxis::Z if (max_z - min_z).abs() >= PHYSICS_EPSILON => {
            ((z - ramp.z1) / (ramp.z2 - ramp.z1)).clamp(0.0, 1.0)
        }
        _ => 0.0,
    };

    ramp.y1 + progress * (ramp.y2 - ramp.y1)
}

#[cfg(test)]
#[path = "tests/levels.rs"]
mod tests;
