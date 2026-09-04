use crate::{constants::PHYSICS_EPSILON, protocol::Ramp};

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
mod tests {
    use super::*;
    use crate::test_geometry::LEVEL_HEIGHT;

    #[test]
    fn ramp_axis_and_surface_follow_longer_x_axis() {
        let ramp = Ramp {
            x1: -4.0,
            y1: 0.0,
            z1: 0.0,
            x2: 4.0,
            y2: LEVEL_HEIGHT,
            z2: 2.0,
        };

        assert_eq!(ramp_axis(&ramp), RampAxis::X);
        assert!((ramp_surface_at(&ramp, 0.0, 0.0) - LEVEL_HEIGHT / 2.0).abs() < PHYSICS_EPSILON);
    }

    #[test]
    fn ramp_axis_and_surface_follow_longer_z_axis() {
        let ramp = Ramp {
            x1: 0.0,
            y1: 0.0,
            z1: -4.0,
            x2: 2.0,
            y2: LEVEL_HEIGHT,
            z2: 4.0,
        };

        assert_eq!(ramp_axis(&ramp), RampAxis::Z);
        assert!((ramp_surface_at(&ramp, 0.0, 0.0) - LEVEL_HEIGHT / 2.0).abs() < PHYSICS_EPSILON);
    }
}
