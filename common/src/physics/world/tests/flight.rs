use crate::{
    map::Carriers,
    physics::{
        CharacterSupport,
        world::{CollisionWorld, tests::wide_body},
    },
    protocol::{CarrierId, Floor, MapLayout, Position, Wall},
};
use bevy_math::Vec3;

#[test]
fn unsupported_flight_holds_altitude_and_moves_in_three_dimensions() {
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let start = Position::from(Vec3::new(4.0, -120.0, 3.0));
    let physics = wide_body();
    let carriers = Carriers::default();
    let idle = world.move_flying_character(start, Vec3::ZERO, 0.1, physics, &[], &carriers);
    assert_eq!(idle.position, start);
    assert_eq!(idle.support, CharacterSupport::Airborne);
    let step = world.move_flying_character(start, Vec3::new(2.0, 3.0, -1.0), 0.1, physics, &[], &carriers);
    assert!(Vec3::from(step.position).distance(Vec3::from(start) + Vec3::new(2.0, 3.0, -1.0)) < 0.001);
}

#[test]
fn flight_sweeps_block_walls_and_ceilings_and_slide_along_them() {
    let world = CollisionWorld::from_map_layout(&MapLayout {
        walls: vec![Wall {
            x1: 0.0,
            z1: -10.0,
            x2: 0.0,
            z2: 10.0,
            y: -10.0,
            height: 20.0,
            width: 0.3,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![Floor {
            x1: -10.0,
            z1: -10.0,
            x2: 10.0,
            z2: 10.0,
            y: 5.0,
            thickness: 0.4,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    });
    let physics = wide_body();
    let start = Position::from(Vec3::new(-3.0, 1.0, 0.0));
    assert!(!world.character_flight_path_clear(start, Position::from(Vec3::new(3.0, 1.0, 0.0)), physics, &[]));
    let step = world.move_flying_character(start, Vec3::new(6.0, 0.0, 2.0), 0.1, physics, &[], &Carriers::default());
    assert!(step.position.x < -1.0);
    assert!(step.position.z > 1.9);
    assert!(!world.character_overlaps_solid(&step.position, physics, &[]));
    let step = world.move_flying_character(start, Vec3::Y * 20.0, 0.1, physics, &[], &Carriers::default());
    assert!(step.position.y < 3.1);
    assert!(!world.character_overlaps_solid(&step.position, physics, &[]));
}

#[test]
fn flying_body_is_pushed_by_rising_geometry_and_crushed_only_when_pinned() {
    use crate::protocol::{Carrier, PlateState};
    for ceiling in [false, true] {
        let mut layout = MapLayout {
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 1,
                from: Position::default(),
                to: Vec3::new(0.0, 6.0, 0.0).into(),
                travel_ticks: 60,
                pause_ticks: 0,
                phase_ticks: 0,
                switch: None,
                switch_inverted: false,
            }],
            floors: vec![Floor {
                x1: -4.0,
                z1: -4.0,
                x2: 4.0,
                z2: 4.0,
                y: 0.0,
                thickness: 0.3,
                level: 0,
                carrier: CarrierId(1),
            }],
            ..Default::default()
        };
        if ceiling {
            layout.floors.push(Floor {
                y: 4.0,
                carrier: CarrierId::WORLD,
                ..layout.floors[0]
            });
        }
        let mut world = CollisionWorld::from_map_layout(&layout);
        let mut carriers = Carriers::from_layout(&layout);
        let mut pos = Position::from(Vec3::Y);
        let mut crushed = false;
        for tick in 1..=40 {
            carriers.advance(tick, &PlateState::default());
            world.set_carrier_poses(&carriers);
            let step = world.move_flying_character(pos, Vec3::ZERO, 1.0 / 30.0, wide_body(), &[], &carriers);
            pos = step.position;
            if step.crushed {
                crushed = true;
                break;
            }
            let mut interior = wide_body();
            interior.movement_collider.diameter -= 0.002;
            interior.movement_collider.height -= 0.002;
            let interior_pos = Position::from(Vec3::from(pos) + Vec3::Y * 0.001);
            assert!(
                !world.character_overlaps_solid(&interior_pos, interior, &[]),
                "penetration at tick {tick}: {step:?}"
            );
        }
        assert_eq!(crushed, ceiling);
        if !ceiling {
            assert!(pos.y > 3.9);
        }
    }
}

#[test]
fn flight_respects_barrier_and_bridge_power_and_ramp_solids() {
    use crate::protocol::{Barrier, BarrierId, BarrierKindId, BridgeId, BridgeKindId, LightBridge, Ramp};
    let physics = wide_body();
    let mut layout = MapLayout {
        barriers: vec![Barrier {
            id: BarrierId(0),
            kind: BarrierKindId(0),
            x1: 0.0,
            z1: -4.0,
            x2: 0.0,
            z2: 4.0,
            y: 0.0,
            height: 5.0,
            width: 0.1,
            levels: 1,
            level: 0,
            carrier: CarrierId::WORLD,
            switch: None,
            switch_inverted: false,
        }],
        light_bridges: vec![LightBridge {
            id: BridgeId(0),
            kind: BridgeKindId(0),
            x1: -8.0,
            z1: -4.0,
            x2: -2.0,
            z2: 4.0,
            y: 4.0,
            thickness: 0.1,
            level: 1,
            carrier: CarrierId::WORLD,
            switch: None,
            switch_inverted: false,
        }],
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout);
    let from = Vec3::new(-2.0, 1.0, 0.0).into();
    let to = Vec3::new(2.0, 1.0, 0.0).into();
    assert!(!world.character_flight_path_clear(from, to, physics, &[]));
    assert!(world.character_flight_path_clear(from, to, physics, &[BarrierId(0)]));
    let below = Vec3::new(-5.0, 0.0, 0.0).into();
    let above = Vec3::new(-5.0, 6.0, 0.0).into();
    assert!(world.character_flight_path_clear(below, above, physics, &[]));
    world.set_powered_bridges(&[BridgeId(0)]);
    assert!(!world.character_flight_path_clear(below, above, physics, &[]));
    layout.ramps.push(Ramp {
        x1: -8.0,
        y1: 0.0,
        z1: -4.0,
        x2: -2.0,
        y2: 4.0,
        z2: 4.0,
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&layout);
    assert!(!world.character_flight_path_clear(below, above, physics, &[]));
}

#[test]
fn stationary_flyer_recovers_from_a_static_wall_overlap() {
    let world = CollisionWorld::from_map_layout(&MapLayout {
        walls: vec![Wall {
            carrier: CarrierId::WORLD,
            level: 0,
            x1: 0.0,
            x2: 0.0,
            z1: -10.0,
            z2: 10.0,
            y: -10.0,
            height: 20.0,
            width: 0.3,
        }],
        ..Default::default()
    });
    let physics = wide_body();
    for x in [-0.1, 0.0, 0.1] {
        let start = Position { x, y: 1.0, z: 0.0 };
        assert!(world.character_overlaps_solid(&start, physics, &[]));
        let step = world.move_flying_character(start, Vec3::ZERO, 0.1, physics, &[], &Carriers::default());
        assert!(!step.crushed);
        assert!(
            !world.character_overlaps_solid(&step.position, physics, &[]),
            "{step:?}"
        );
        let target = Position {
            z: 3.0,
            ..step.position
        };
        assert!(world.character_flight_path_clear(step.position, target, physics, &[]));
    }
}
