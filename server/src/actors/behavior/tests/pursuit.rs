use super::super::pursuit::pursuit_surface;
use super::*;
use common::protocol::{Carrier, CarrierMotion, Floor, LightBridge};

#[test]
fn airborne_pursuit_uses_the_highest_enabled_surface_in_its_current_carrier_frame() {
    let carried = CarrierId::from_carried_index(0);
    let field = FieldId(0);
    let floor = |carrier, half_width: f32| Floor {
        x1: -half_width,
        x2: half_width,
        z1: -half_width,
        z2: half_width,
        y: 0.0,
        thickness: 0.2,
        level: 0,
        carrier,
    };
    let layout = MapLayout {
        floors: vec![floor(CarrierId::WORLD, 30.0), floor(carried, 2.0)],
        light_bridges: vec![LightBridge {
            x1: -2.0,
            x2: 2.0,
            z1: -2.0,
            z2: 2.0,
            y: 2.0,
            thickness: 0.2,
            level: 1,
            field,
            carrier: carried,
        }],
        carriers: vec![Carrier {
            initially_on: true,
            motion: CarrierMotion::Cycle,
            parent: CarrierId::WORLD,
            level: 0,
            levels: 1,
            from: Position {
                x: 10.0,
                y: 2.0,
                z: 0.0,
            },
            to: Position {
                x: 14.0,
                y: 3.0,
                z: 0.0,
            },
            travel_ticks: 60,
            pause_ticks: 0,
            phase_ticks: 0,
            switch: None,
        }],
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout);
    let mut carriers = Carriers::from_layout(&layout);
    for tick in [0, 30, 60] {
        carriers.advance(tick, &SwitchState::default());
        world.set_carrier_poses(&carriers);
        let position = carriers.pose(carried).transform_position(&Position {
            x: 0.5,
            y: 12.0,
            z: 0.5,
        });
        let airborne = aware(1, position, CharacterSupport::Airborne, true);
        for (open, y) in [(vec![], 2.0), (vec![field], 0.0)] {
            let goal = pursuit_surface(&airborne, &world, &carriers, &open).expect("surface below");
            assert_eq!(goal.carrier, carried);
            assert!(
                goal.position.distance_sq(&Position { x: 0.5, y, z: 0.5 }) < 1e-6,
                "{goal:?}"
            );
        }
        let below_bridge = aware(
            1,
            Position {
                y: position.y - 11.0,
                ..position
            },
            CharacterSupport::Airborne,
            true,
        );
        let goal = pursuit_surface(&below_bridge, &world, &carriers, &[]).expect("floor below bridge");
        assert_eq!(goal.carrier, carried);
        assert!(goal.position.y.abs() < 1e-4);
        let supported = aware(1, position, CharacterSupport::Ground, true);
        let goal = pursuit_surface(&supported, &world, &carriers, &[]).expect("reported supported position");
        assert_eq!(
            goal.position, position,
            "supported targets must not be projected through floors"
        );
    }
}
