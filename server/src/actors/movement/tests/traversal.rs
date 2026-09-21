use super::*;
use crate::actors::navigation::surface::{RouteFailure, SurfaceMesh, fixtures};
use common::{
    physics::{CharacterMovePlan, character_move_plans_intersect},
    protocol::{FieldId, Floor, MapLayout, Wall},
};

fn flat_world(walls: Vec<Wall>) -> (CollisionWorld, Carriers) {
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -8.0,
            x2: 8.0,
            z1: -8.0,
            z2: 8.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        walls,
        ..Default::default()
    };
    (CollisionWorld::from_map_layout(&layout), Carriers::from_layout(&layout))
}

// Steps a walker past a body standing at the origin, returning its closest
// approach to it.
fn walk_against_body(actor: &mut TraversalExecutor, env: &TraversalEnvironment, ticks: usize) -> f32 {
    let body = CharacterMovePlan::stationary(Entity::from_bits(2), Position::default(), 0.0, actor.physics);
    let mut closest = f32::INFINITY;
    for _ in 0..ticks {
        let from = actor.movement.position;
        let physics = actor.physics;
        actor.step_with_avoidance(env, Vec3::ZERO, Vec3::ZERO, false, None, |movement| {
            let plan = CharacterMovePlan::from_movement_result(Entity::from_bits(1), from, *movement, physics);
            character_move_plans_intersect(&plan, &body).then_some(BodyBlocker {
                entity: body.entity,
                offset: Vec3::from(body.start) - Vec3::from(from),
            })
        });
        closest = closest.min(actor.movement.position.horizontal_distance_sq(&body.start).sqrt());
        if actor.status != TraversalStatus::Moving {
            break;
        }
    }
    closest
}

fn walker(env: &TraversalEnvironment, target: Position) -> TraversalExecutor {
    let physics = fixtures::config().expect_actor("scuttler").character.physics();
    let start = Position {
        x: -3.0,
        y: 0.0,
        z: 0.0,
    };
    let mut actor = TraversalExecutor::new(start, physics, 2.0, env);
    actor.set_route(SurfaceRoute {
        actions: [TraversalAction::Walk {
            carrier: CarrierId::WORLD,
            target,
        }]
        .into(),
        expanded: 0,
    });
    actor.facing = std::f32::consts::FRAC_PI_2;
    actor
}

#[test]
fn a_walker_whose_target_lies_under_another_body_reports_blocked_instead_of_circling() {
    let config = fixtures::config();
    let (world, carriers) = flat_world(Vec::new());
    let env = TraversalEnvironment {
        world: &world,
        carriers: &carriers,
        settings: &config.settings,
        open: &[],
        delta: 1.0 / 30.0,
    };
    let mut actor = walker(&env, Position { x: 0.1, y: 0.0, z: 0.1 });
    let closest = walk_against_body(&mut actor, &env, 300);
    assert_eq!(actor.status, TraversalStatus::Blocked, "{actor:?}");
    assert_eq!(actor.blocked_by, Some(Entity::from_bits(2)));
    assert!(closest >= actor.physics.movement_collider.diameter - 1e-3, "{closest}");
}

#[test]
fn a_walker_passes_a_body_on_the_open_side_when_a_wall_closes_its_usual_hand() {
    let config = fixtures::config();
    // The first sidestep from a walk along +X turns toward -Z.
    let (world, carriers) = flat_world(vec![Wall {
        x1: -8.0,
        z1: -0.5,
        x2: 8.0,
        z2: -0.5,
        width: 0.3,
        y: 0.0,
        height: 1.5,
        level: 0,
        carrier: CarrierId::WORLD,
    }]);
    let env = TraversalEnvironment {
        world: &world,
        carriers: &carriers,
        settings: &config.settings,
        open: &[],
        delta: 1.0 / 30.0,
    };
    let mut actor = walker(&env, Position { x: 4.0, y: 0.0, z: 0.0 });
    let closest = walk_against_body(&mut actor, &env, 300);
    assert_eq!(actor.status, TraversalStatus::Reached, "{actor:?}");
    assert!(closest >= actor.physics.movement_collider.diameter - 1e-3, "{closest}");
}

#[test]
fn a_pivot_that_only_its_travel_probe_blocks_keeps_its_knockback() {
    let config = fixtures::config();
    let (world, carriers) = flat_world(Vec::new());
    let env = TraversalEnvironment {
        world: &world,
        carriers: &carriers,
        settings: &config.settings,
        open: &[],
        delta: 1.0 / 30.0,
    };
    let mut actor = walker(&env, Position { x: 4.0, y: 0.0, z: 0.0 });
    actor.facing = -std::f32::consts::FRAC_PI_2;
    let start = actor.movement.position;
    let impulse = Vec3::new(0.0, 0.0, 0.1);
    let expected = step_actor_movement(ActorMovementStep {
        start,
        vertical_velocity: actor.movement.vertical_velocity,
        intent: ActorMoveIntent::Idle,
        external_displacement: impulse,
        delta: env.delta,
        can_use_ladders: false,
        physics: actor.physics,
        open_fields: &[],
        collision_world: &world,
        map_settings: &config.settings,
        carriers: &carriers,
    });
    // Every voluntary move is rejected; the pivot itself travels nowhere.
    actor.step_with_avoidance(&env, impulse, Vec3::ZERO, false, None, |movement| {
        (movement.position.x != expected.position.x).then_some(BodyBlocker {
            entity: Entity::from_bits(2),
            offset: Vec3::X,
        })
    });
    assert_eq!(actor.intent.speed(), Some(0.0));
    assert_eq!(actor.movement, expected);
}

#[test]
fn actor_blocking_rejects_a_swept_impulse_and_preserves_the_full_carried_landing() {
    use common::protocol::{Carrier, CarrierMotion, SwitchState};

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
            from: Position::default(),
            to: Position {
                x: 30.0,
                y: 3.0,
                z: 0.0,
            },
            travel_ticks: 30,
            pause_ticks: 0,
            phase_ticks: 0,
            switch: None,
        }],
        floors: vec![Floor {
            x1: -6.0,
            x2: 6.0,
            z1: -3.0,
            z2: 3.0,
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
    let start = Position {
        x: -2.0,
        y: 0.0,
        z: 0.0,
    };
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
    actor.movement.position = start;
    actor.movement.vertical_velocity = -3.0;
    carriers.advance(1, &SwitchState::default());
    world.set_carrier_poses(&carriers);
    let env = TraversalEnvironment {
        world: &world,
        carriers: &carriers,
        settings: &config.settings,
        open: &[],
        delta: 1.0 / 30.0,
    };
    let expected = step_actor_movement(ActorMovementStep {
        start,
        vertical_velocity: -3.0,
        intent: ActorMoveIntent::Idle,
        external_displacement: Vec3::ZERO,
        delta: env.delta,
        can_use_ladders: false,
        physics,
        open_fields: &[],
        collision_world: &world,
        map_settings: &config.settings,
        carriers: &carriers,
    });
    let other = CharacterMovePlan::from_target(
        Entity::from_bits(2),
        Position::default(),
        carriers.pose(carrier).transform_position(&Position::default()),
        0.0,
        physics,
        false,
    );
    let blocks = |movement: &CharacterMovementResult| {
        let plan = CharacterMovePlan::from_movement_result(Entity::from_bits(1), start, *movement, physics);
        character_move_plans_intersect(&plan, &other).then_some(BodyBlocker {
            entity: other.entity,
            offset: Vec3::from(other.start) - Vec3::from(start),
        })
    };
    let mut unblocked = actor.clone();
    unblocked.step_with_avoidance(&env, Vec3::X * 5.0, Vec3::ZERO, false, None, |_| None);
    assert!(
        unblocked.movement.position.x > other.target.x + physics.movement_collider.diameter,
        "the impulse crosses the body and ends clear"
    );
    actor.step_with_avoidance(&env, Vec3::X * 5.0, Vec3::ZERO, false, None, blocks);
    assert_eq!(actor.movement, expected);
    assert_eq!(actor.movement.support, CharacterSupport::Ground);
    assert_eq!(actor.movement.carrier, carrier);
    assert!(actor.movement.impact_speed > 0.0);
    assert!(actor.movement.position.x > start.x, "carrier motion is retained");
}

#[test]
fn roaming_rejects_crowd_steering_out_of_a_moving_home_but_keeps_physics() {
    use crate::map::ZoneVolume;
    use common::protocol::{Carrier, CarrierMotion, SwitchState};

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
    unrestricted.step_with_avoidance(&env, Vec3::ZERO, Vec3::X, false, None, |_| None);
    assert!(
        !home.contains_position(
            carriers
                .pose(carrier)
                .inverse_transform_point(unrestricted.movement.position.into())
        )
    );

    let mut confined = actor.clone();
    confined.step_with_avoidance(&env, Vec3::ZERO, Vec3::X, false, Some(&home), |_| None);
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
    actor.step_with_avoidance(&env, impulse, Vec3::ZERO, false, Some(&home), |_| None);
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
