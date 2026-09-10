use super::*;
use crate::{protocol::CarrierId, test_geometry::LEVEL_HEIGHT};

#[test]
fn ramp_axis_and_surface_follow_longer_x_axis() {
    let ramp = Ramp {
        x1: -4.0,
        y1: 0.0,
        z1: 0.0,
        x2: 4.0,
        y2: LEVEL_HEIGHT,
        z2: 2.0,
        carrier: CarrierId::WORLD,
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
        carrier: CarrierId::WORLD,
    };

    assert_eq!(ramp_axis(&ramp), RampAxis::Z);
    assert!((ramp_surface_at(&ramp, 0.0, 0.0) - LEVEL_HEIGHT / 2.0).abs() < PHYSICS_EPSILON);
}
