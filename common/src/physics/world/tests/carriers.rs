use rapier3d::{control::KinematicCharacterController, prelude::Vector};

use super::*;
use crate::{
    config::gameplay::load_test_gameplay, constants::TICK_SECS, map::Carriers,
    physics::characters::character_movement_pose, protocol::PlateState,
};

#[test]
fn carrier_colliders_follow_the_carrier_pose() {
    let layout = slider_layout();
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    assert_eq!(world.solid_kinds(), vec![ColliderKind::Floor]);
    let shape = character_movement_shape(wide_body());
    let probe = |world: &CollisionWorld, x: f32| {
        let pose = Pose::translation(x, LEVEL_HEIGHT + 0.0 + 0.05, 0.0);
        world.ground_hit(&shape, &pose, 1.0, 0.0, &[], &[])
    };

    assert!(probe(&world, 0.0).is_some(), "the tile starts at its first end");
    let mut carriers = Carriers::from_layout(&layout);
    carriers.advance(60, &PlateState::default());
    world.set_carrier_poses(&carriers);
    assert!(probe(&world, 0.0).is_none(), "the tile left its first end");
    assert!(probe(&world, 8.0).is_some(), "the tile arrived at its second end");
}

#[test]
fn ground_hit_names_the_carrier_under_the_feet() {
    let mut layout = slider_layout();
    layout.floors.push(Floor {
        x1: 4.0,
        z1: -2.0,
        x2: 12.0,
        z2: 2.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let shape = character_movement_shape(wide_body());
    let probe = |x: f32| {
        let pose = Pose::translation(x, LEVEL_HEIGHT + 0.0 + 0.05, 0.0);
        world
            .ground_hit(&shape, &pose, 1.0, 0.0, &[], &[])
            .map(|hit| hit.carrier)
    };

    assert_eq!(probe(0.0), Some(CarrierId(1)));
    assert_eq!(probe(8.0), Some(CarrierId::WORLD));
}

#[test]
fn world_surface_ray_names_the_carrier_it_hits() {
    let mut layout = slider_layout();
    layout.floors.push(Floor {
        x1: -2.0,
        z1: -2.0,
        x2: 2.0,
        z2: 2.0,
        y: LEVEL_HEIGHT - 2.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let above = Vec3::new(0.0, LEVEL_HEIGHT + 1.0, 0.0);

    let hit = world
        .world_surface_along_ray(above, Vec3::NEG_Y, 4.0)
        .expect("the tile did not stop the ray");
    assert!((hit.point.y - LEVEL_HEIGHT).abs() < 1e-3, "hit was {hit:?}");
    assert_eq!(hit.carrier, CarrierId(1));

    let beside = Vec3::new(1.8, LEVEL_HEIGHT + 1.0, 0.0);
    let hit = world
        .world_surface_along_ray(beside, Vec3::NEG_Y, 4.0)
        .expect("the floor under the tile's edge was missed");
    assert!((hit.point.y - (LEVEL_HEIGHT - 2.0)).abs() < 1e-3, "hit was {hit:?}");
    assert_eq!(hit.carrier, CarrierId::WORLD);
}

#[test]
fn carrier_pushes_respect_barrier_passability_bridge_power_and_portal_exclusions() {
    let carrier = CarrierId(1);
    let wall = Wall {
        x1: -2.0,
        x2: 2.0,
        z1: 0.0,
        z2: 0.0,
        width: 0.4,
        y: 0.0,
        height: 3.0,
        level: 0,
        carrier,
    };
    let barrier = Barrier {
        x1: wall.x1,
        x2: wall.x2,
        z1: wall.z1,
        z2: wall.z2,
        width: wall.width,
        y: wall.y,
        height: wall.height,
        level: 0,
        levels: 1,
        kind: BarrierKindId(0),
        carrier,
    };
    let bridge = LightBridge {
        x1: -2.0,
        x2: 2.0,
        z1: -2.0,
        z2: 0.2,
        y: 1.5,
        thickness: 1.0,
        level: 0,
        kind: BridgeKindId(0),
        carrier,
    };
    let table = BarrierKindTable::from_ids(vec!["red".into()]).expect("barrier catalog invalid");
    let physics = load_test_gameplay()
        .expect("test gameplay config invalid")
        .player
        .physics();
    let shape = character_movement_shape(physics);
    let pose = character_movement_pose(
        &Position {
            x: 0.0,
            y: 0.0,
            z: 0.2 + physics.movement_collider.radius() + 0.01,
        },
        physics,
    );
    for kind in [ColliderKind::Wall, ColliderKind::Barrier, ColliderKind::Bridge] {
        let layout = MapLayout {
            walls: if kind == ColliderKind::Wall { vec![wall] } else { vec![] },
            barriers: if kind == ColliderKind::Barrier {
                vec![barrier]
            } else {
                vec![]
            },
            light_bridges: if kind == ColliderKind::Bridge {
                vec![bridge]
            } else {
                vec![]
            },
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 0,
                from: Position::default(),
                to: Position { x: 0.0, y: 0.0, z: 4.0 },
                travel_ticks: 60,
                pause_ticks: 0,
                phase_ticks: 0,
                switch: None,
            }],
            ..Default::default()
        };
        let mut world = CollisionWorld::from_map_layout(&layout, &table);
        let mut carriers = Carriers::from_layout(&layout);
        carriers.advance(1, &PlateState::default());
        world.set_carrier_poses(&carriers);
        let handles: Vec<_> = world.colliders.iter().map(|(handle, _)| handle).collect();
        for powered in [false, true] {
            world.set_powered_bridges(if powered { &[BridgeKindId(0)] } else { &[] });
            for passable in [vec![], vec![BarrierKindId(0)]] {
                for excluded in [&[][..], handles.as_slice()] {
                    let movement = world.push_character_from_carriers(
                        TICK_SECS,
                        &KinematicCharacterController::default(),
                        &shape,
                        &pose,
                        &carriers,
                        &passable,
                        excluded,
                        |_| {},
                    );
                    let solid = excluded.is_empty()
                        && (kind != ColliderKind::Barrier || passable.is_empty())
                        && (kind != ColliderKind::Bridge || powered);
                    assert_eq!(
                        movement.z > 0.05,
                        solid,
                        "kind {kind:?}, powered {powered}, passable {passable:?}, excluded {excluded:?}: {movement:?}"
                    );
                    if !solid {
                        assert_eq!(movement, Vector::ZERO);
                    }
                }
            }
        }
    }
}
