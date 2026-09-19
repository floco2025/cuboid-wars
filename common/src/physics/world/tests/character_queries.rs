use crate::{
    physics::world::{CollisionWorld, tests::wide_body},
    protocol::{CarrierId, MapLayout, Position, Ramp, RampDirection, RampShape},
};

#[test]
fn ground_route_clearance_accepts_ramp_surface_positions_in_both_directions() {
    let world = CollisionWorld::from_map_layout(&MapLayout {
        ramps: vec![Ramp {
            x1: -4.0,
            z1: -2.0,
            x2: 4.0,
            z2: 2.0,
            y: 0.0,
            height: 2.0,
            direction: RampDirection::East,
            shape: RampShape::Solid,
            thickness: 0.4,
            level: 0,
            levels: 1,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    });
    let lower = Position {
        x: -3.0,
        y: 0.25,
        z: 0.0,
    };
    let upper = Position {
        x: 3.0,
        y: 1.75,
        z: 0.0,
    };
    for (start, end) in [(lower, upper), (upper, lower), (lower, lower)] {
        assert!(
            world.character_ground_route_clear(start, end, wide_body(), &[]),
            "{start:?} -> {end:?}"
        );
    }
}
