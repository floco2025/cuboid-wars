use super::*;
use crate::test_fixtures::structural_solids;
use common::protocol::{Floor, MapLayout, Ramp, RampDirection, RampShape, Wall};

fn clearance(layout: &MapLayout, carrier: CarrierId) -> GrassClearance {
    GrassClearance::new(&structural_solids(layout), carrier)
}

fn ramp() -> Ramp {
    Ramp {
        x1: 0.0,
        x2: 10.0,
        z1: -2.0,
        z2: 2.0,
        y: 0.0,
        height: 4.0,
        thickness: 0.2,
        direction: RampDirection::East,
        shape: RampShape::Plank,
        levels: 1,
        carrier: CarrierId::WORLD,
        level: 0,
    }
}

#[test]
fn blades_clear_the_low_end_of_planks_but_grow_under_the_high_end() {
    let mut layout = MapLayout {
        ramps: vec![ramp()],
        ..default()
    };
    let plank = clearance(&layout, CarrierId::WORLD);
    assert!(!plank.allows(Vec3::new(0.2, 0.0, 0.0)));
    assert!(!plank.allows(Vec3::new(3.0, 1.0, 0.0))); // rising grounds under the plank
    assert!(plank.allows(Vec3::new(8.0, 0.0, 0.0)));
    assert!(plank.allows(Vec3::new(0.2, 0.0, 4.0)));
    layout.ramps[0].shape = RampShape::Solid;
    assert!(!clearance(&layout, CarrierId::WORLD).allows(Vec3::new(8.0, 0.0, 0.0)));
    assert!(clearance(&layout, CarrierId(1)).allows(Vec3::new(8.0, 0.0, 0.0)));
}

#[test]
fn support_floors_allow_blades_but_low_overhead_slabs_exclude_them() {
    let mut layout = MapLayout {
        floors: vec![Floor {
            x1: -5.0,
            x2: 5.0,
            z1: -5.0,
            z2: 5.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    assert!(clearance(&layout, CarrierId::WORLD).allows(Vec3::ZERO));
    layout.floors.push(Floor {
        y: 0.4,
        ..layout.floors[0]
    });
    assert!(!clearance(&layout, CarrierId::WORLD).allows(Vec3::ZERO));
    assert!(clearance(&layout, CarrierId::WORLD).allows(Vec3::Y * 0.4));
}

// A blade beside a wall only leans into its face; one inside it never shows.
#[test]
fn blades_grow_up_to_a_wall_and_not_inside_it() {
    let layout = MapLayout {
        walls: vec![Wall {
            x1: -5.0,
            x2: 5.0,
            z1: 1.0,
            z2: 1.0,
            width: 0.2,
            y: 0.0,
            height: 3.0,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    let wall = clearance(&layout, CarrierId::WORLD);
    assert!(wall.allows(Vec3::new(0.0, 0.0, 0.85)));
    assert!(!wall.allows(Vec3::new(0.0, 0.0, 1.0)));
    assert!(wall.allows(Vec3::new(0.0, 0.0, 1.15)));
}
