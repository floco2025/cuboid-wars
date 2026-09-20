use super::*;
use common::protocol::RampDirection;

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
    let clearance = GrassClearance::new(&layout, CarrierId::WORLD);
    assert!(!clearance.allows(Vec3::new(0.2, 0.0, 0.0)));
    assert!(!clearance.allows(Vec3::new(3.0, 1.0, 0.0))); // rising grounds under the plank
    assert!(clearance.allows(Vec3::new(8.0, 0.0, 0.0)));
    assert!(clearance.allows(Vec3::new(0.2, 0.0, 4.0)));
    layout.ramps[0].shape = RampShape::Solid;
    assert!(!GrassClearance::new(&layout, CarrierId::WORLD).allows(Vec3::new(8.0, 0.0, 0.0)));
    assert!(GrassClearance::new(&layout, CarrierId(1)).allows(Vec3::new(8.0, 0.0, 0.0)));
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
    assert!(GrassClearance::new(&layout, CarrierId::WORLD).allows(Vec3::ZERO));
    layout.floors.push(Floor {
        y: 0.4,
        ..layout.floors[0]
    });
    assert!(!GrassClearance::new(&layout, CarrierId::WORLD).allows(Vec3::ZERO));
    assert!(GrassClearance::new(&layout, CarrierId::WORLD).allows(Vec3::Y * 0.4));
}
