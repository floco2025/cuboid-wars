use super::*;
use crate::actors::navigation::surface::{RouteFailure, SurfaceMesh, fixtures};
use common::protocol::FieldId;

#[test]
fn roaming_rejects_crowd_steering_out_of_a_moving_home_but_keeps_physics() {
    use crate::map::ZoneVolume;
    use common::protocol::{Carrier, CarrierMotion, Floor, MapLayout, SwitchState};

    let config = fixtures::config();
    let physics = config.expect_actor("scuttler").character.physics();
    let carrier = CarrierId(1);
    let layout = MapLayout {
        carriers: vec![Carrier {
            initially_on: true,
            motion: CarrierMotion::Cycle,
            parent: CarrierId::WORLD,
            level: 0,
            levels: 1,
            from: Position {
                x: 20.0,
                y: 0.0,
                z: 0.0,
            },
            to: Position {
                x: 30.0,
                y: 3.0,
                z: 0.0,
            },
            travel_ticks: 120,
            pause_ticks: 0,
            phase_ticks: 0,
            switch: None,
        }],
        floors: vec![Floor {
            x1: -4.0,
            x2: 4.0,
            z1: -4.0,
            z2: 4.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier,
        }],
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout);
    let mut carriers = Carriers::from_layout(&layout);
    world.set_carrier_poses(&carriers);
    let home = ActorTerritory {
        carrier,
        volume: ZoneVolume {
            min: Vec3::new(-1.0, 0.0, -3.0),
            max: Vec3::new(1.0, 2.0, 3.0),
        },
        distance: 0.0,
        center_height: physics.movement_collider.height / 2.0,
    };
    let start = carriers
        .pose(carrier)
        .transform_point(Vec3::new(0.999, 0.0, 0.0))
        .into();
    let mut actor = TraversalExecutor::new(
        start,
        physics,
        2.0,
        &TraversalEnvironment {
            world: &world,
            carriers: &carriers,
            settings: &config.settings,
            open: &[],
            delta: 1.0 / 30.0,
        },
    );
    actor.actions.push_back(TraversalAction::Walk {
        carrier,
        target: Position {
            x: 0.999,
            y: 0.0,
            z: 2.0,
        },
    });
    carriers.advance(1, &SwitchState::default());
    world.set_carrier_poses(&carriers);
    let env = TraversalEnvironment {
        world: &world,
        carriers: &carriers,
        settings: &config.settings,
        open: &[],
        delta: 1.0 / 30.0,
    };
    let mut unrestricted = actor.clone();
    unrestricted.step_with_avoidance(&env, Vec3::ZERO, Vec3::X, false, None);
    assert!(
        !home.contains_position(
            carriers
                .pose(carrier)
                .inverse_transform_point(unrestricted.movement.position.into())
        )
    );

    let mut confined = actor.clone();
    confined.step_with_avoidance(&env, Vec3::ZERO, Vec3::X, false, Some(&home));
    assert_eq!(confined.status, TraversalStatus::OutsideTerritory);
    assert_eq!(
        confined.intent.direction(),
        unrestricted.intent.direction(),
        "keep turning toward safe ground"
    );
    assert!(confined.actions.is_empty());
    assert!(
        home.contains_position(
            carriers
                .pose(carrier)
                .inverse_transform_point(confined.movement.position.into())
        )
    );
    assert_eq!(confined.movement.support, CharacterSupport::Ground);
    assert_eq!(confined.movement.carrier, carrier);
    assert!(confined.movement.position.x > start.x, "carrier travel is retained");

    let impulse = Vec3::X * 0.2;
    let expected = step_actor_movement(ActorMovementStep {
        start: actor.movement.position,
        vertical_velocity: actor.movement.vertical_velocity,
        intent: ActorMoveIntent::Idle,
        external_displacement: impulse,
        delta: env.delta,
        can_use_ladders: false,
        physics,
        open_fields: &[],
        collision_world: &world,
        map_settings: &config.settings,
        carriers: &carriers,
    });
    actor.step_with_avoidance(&env, impulse, Vec3::ZERO, false, Some(&home));
    assert_eq!(
        actor.movement, expected,
        "physical displacement is not clamped at the territory boundary"
    );
}

#[test]
fn removing_a_bridge_stops_an_existing_route_at_the_edge_until_support_returns() {
    let config = fixtures::config();
    let generated = fixtures::generate("fixture", 30, &config.settings).expect("authored bridge scene");
    let world = CollisionWorld::from_map_layout(&generated.layout);
    let carriers = Carriers::from_layout(&generated.layout);
    let geometry = world.collision_meshes().expect("collision export");
    let physics = config.expect_actor("scuttler").character.physics();
    let start = Position { x: 4.5, y: 3.0, z: 0.0 };
    let goal = Position {
        x: 13.5,
        y: 3.0,
        z: 0.0,
    };
    let mesh = SurfaceMesh::bake(&geometry, CarrierId::WORLD, physics, &[]).expect("mesh bake");
    let route = mesh.route(start, goal, 0.7).expect("bridge connects the islands");
    let open = [FieldId(0)];
    let disconnected = SurfaceMesh::bake(&geometry, CarrierId::WORLD, physics, &open).expect("mesh without bridge");
    assert_eq!(
        disconnected.route(start, goal, 0.7).expect_err("disabled bridge"),
        RouteFailure::Disconnected
    );
    let mut env = TraversalEnvironment {
        world: &world,
        carriers: &carriers,
        settings: &config.settings,
        open: &[],
        delta: 1.0 / 30.0,
    };
    let mut actor = TraversalExecutor::new(start, physics, 3.0, &env);
    actor.set_route(route);
    for _ in 0..5 {
        actor.step(&env);
    }
    env.open = &open;
    for _ in 0..90 {
        actor.step(&env);
    }
    assert_eq!(actor.status, TraversalStatus::LostSupport);
    assert_eq!(actor.movement.support, CharacterSupport::Ground);
    assert!(
        (actor.movement.position.y - start.y).abs() < 0.1,
        "{:?}",
        actor.movement
    );
    env.open = &[];
    for _ in 0..150 {
        actor.step(&env);
    }
    assert_eq!(actor.status, TraversalStatus::Reached, "{actor:?}");
    assert!(actor.movement.position.distance_sq(&goal) < 0.1);
}
