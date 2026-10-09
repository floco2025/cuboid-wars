use super::*;
use crate::{
    actors::navigation::surface::{RouteFailure, SurfaceMesh, fixtures},
    map::ZoneVolume,
};
use common::{
    physics::{CharacterMovePlan, character_move_plans_intersect},
    protocol::{Carrier, CarrierMotion, FieldId, Floor, MapLayout, SwitchState, Wall},
};

fn flat_world(walls: Vec<Wall>) -> (CollisionWorld, Carriers) {
    let layout = MapLayout {
        floors: vec![fixtures::floor([-8.0, 8.0], [-8.0, 8.0], 0.0)],
        walls,
        ..Default::default()
    };
    (CollisionWorld::from_map_layout(&layout), Carriers::from_layout(&layout))
}

// Steps a walker past a body standing at the origin, returning its closest
// approach to it.
fn walk_against_body(
    actor: &mut TraversalExecutor,
    walker: &mut ActorBody,
    env: &TraversalEnvironment,
    ticks: usize,
) -> f32 {
    let body = CharacterMovePlan::stationary(Entity::from_bits(2), Position::default(), 0.0, actor.physics);
    let mut closest = f32::INFINITY;
    for _ in 0..ticks {
        let from = walker.position;
        let physics = actor.physics;
        let movement = actor.step_with_avoidance(env, *walker, Vec3::ZERO, Vec3::ZERO, false, None, |target| {
            let plan = CharacterMovePlan::from_target(Entity::from_bits(1), from, target, 0.0, physics, false);
            character_move_plans_intersect(&plan, &body).then_some(BodyBlocker {
                entity: body.entity,
                offset: Vec3::from(body.start) - Vec3::from(from),
            })
        });
        *walker = ActorBody::from(&movement);
        closest = closest.min(walker.position.horizontal_distance_sq(&body.start).sqrt());
        if actor.status != TraversalStatus::Moving {
            break;
        }
    }
    closest
}

// Where the motor alone takes an idle body this tick.
fn idle_step(
    env: &TraversalEnvironment,
    physics: CharacterPhysicsConfig,
    body: ActorBody,
    knockback: Vec3,
) -> CharacterMovementResult {
    step_actor_movement(ActorMovementStep {
        start: body.position,
        vertical_velocity: body.vertical_velocity,
        intent: ActorMoveIntent::Idle,
        knockback_displacement: knockback,
        delta: env.delta,
        can_use_ladders: false,
        physics,
        open_fields: &[],
        collision_world: env.world,
        map_settings: env.settings,
        carriers: env.carriers,
    })
}

const WALKER_START: Position = Position {
    x: -3.0,
    y: 0.0,
    z: 0.0,
};

fn walker(target: Position) -> TraversalExecutor {
    let physics = fixtures::config().expect_actor("scuttler").character.physics();
    let mut actor = TraversalExecutor::new(physics, 2.0);
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
    let env = fixtures::env(&world, &carriers, &config);
    let mut actor = walker(Position { x: 0.1, y: 0.0, z: 0.1 });
    let mut body = ActorBody::standing(WALKER_START);
    let closest = walk_against_body(&mut actor, &mut body, &env, 300);
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
    let env = fixtures::env(&world, &carriers, &config);
    let mut actor = walker(Position { x: 4.0, y: 0.0, z: 0.0 });
    let mut body = ActorBody::standing(WALKER_START);
    let closest = walk_against_body(&mut actor, &mut body, &env, 300);
    assert_eq!(actor.status, TraversalStatus::Reached, "{actor:?}");
    assert!(closest >= actor.physics.movement_collider.diameter - 1e-3, "{closest}");
}

#[test]
fn a_pivot_that_only_its_travel_probe_blocks_keeps_its_knockback() {
    let config = fixtures::config();
    let (world, carriers) = flat_world(Vec::new());
    let env = fixtures::env(&world, &carriers, &config);
    let mut actor = walker(Position { x: 4.0, y: 0.0, z: 0.0 });
    actor.facing = -std::f32::consts::FRAC_PI_2;
    let body = ActorBody::standing(WALKER_START);
    let impulse = Vec3::new(0.0, 0.0, 0.1);
    let expected = idle_step(&env, actor.physics, body, impulse);
    // Every voluntary move is rejected; the pivot itself travels nowhere.
    let movement = actor.step_with_avoidance(&env, body, impulse, Vec3::ZERO, false, None, |target| {
        (target.x != expected.position.x).then_some(BodyBlocker {
            entity: Entity::from_bits(2),
            offset: Vec3::X,
        })
    });
    assert_eq!(actor.intent.speed(), Some(0.0));
    assert_eq!(movement, expected);
}

#[test]
fn actor_blocking_rejects_a_swept_impulse_and_preserves_the_full_carried_landing() {
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
            carrier,
            ..fixtures::floor([-6.0, 6.0], [-3.0, 3.0], 0.0)
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
    let mut actor = TraversalExecutor::new(physics, 2.0);
    let body = ActorBody {
        position: start,
        vertical_velocity: -3.0,
        support: CharacterSupport::Airborne,
    };
    carriers.advance(1, &SwitchState::default());
    world.set_carrier_poses(&carriers);
    let env = fixtures::env(&world, &carriers, &config);
    let expected = idle_step(&env, physics, body, Vec3::ZERO);
    let other = CharacterMovePlan::from_target(
        Entity::from_bits(2),
        Position::default(),
        carriers.pose(carrier).transform_position(&Position::default()),
        0.0,
        physics,
        false,
    );
    let blocks = |target: Position| {
        let plan = CharacterMovePlan::from_target(Entity::from_bits(1), start, target, 0.0, physics, false);
        character_move_plans_intersect(&plan, &other).then_some(BodyBlocker {
            entity: other.entity,
            offset: Vec3::from(other.start) - Vec3::from(start),
        })
    };
    let unblocked = actor
        .clone()
        .step_with_avoidance(&env, body, Vec3::X * 5.0, Vec3::ZERO, false, None, |_| None);
    assert!(
        unblocked.position.x > other.target.x + physics.movement_collider.diameter,
        "the impulse crosses the body and ends clear"
    );
    let movement = actor.step_with_avoidance(&env, body, Vec3::X * 5.0, Vec3::ZERO, false, None, blocks);
    assert_eq!(movement, expected);
    assert_eq!(movement.support, CharacterSupport::Ground);
    assert_eq!(movement.carrier, carrier);
    assert!(movement.impact_speed > 0.0);
    assert!(movement.position.x > start.x, "carrier motion is retained");
}

#[test]
fn roaming_rejects_crowd_steering_out_of_a_moving_home_but_keeps_physics() {
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
            carrier,
            ..fixtures::floor([-4.0, 4.0], [-4.0, 4.0], 0.0)
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
    let mut actor = TraversalExecutor::new(physics, 2.0);
    let body = ActorBody::standing(start);
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
    let env = fixtures::env(&world, &carriers, &config);
    let mut unrestricted = actor.clone();
    let free = unrestricted.step_with_avoidance(&env, body, Vec3::ZERO, Vec3::X, false, None, |_| None);
    assert!(!home.contains_position(carriers.pose(carrier).inverse_transform_point(free.position.into())));

    let mut confined = actor.clone();
    let kept = confined.step_with_avoidance(&env, body, Vec3::ZERO, Vec3::X, false, Some(&home), |_| None);
    assert_eq!(confined.status, TraversalStatus::OutsideTerritory);
    assert_eq!(
        confined.intent.direction(),
        unrestricted.intent.direction(),
        "keep turning toward safe ground"
    );
    assert!(confined.actions.is_empty());
    assert!(home.contains_position(carriers.pose(carrier).inverse_transform_point(kept.position.into())));
    assert_eq!(kept.support, CharacterSupport::Ground);
    assert_eq!(kept.carrier, carrier);
    assert!(kept.position.x > start.x, "carrier travel is retained");

    let impulse = Vec3::X * 0.2;
    let expected = idle_step(&env, physics, body, impulse);
    let movement = actor.step_with_avoidance(&env, body, impulse, Vec3::ZERO, false, Some(&home), |_| None);
    assert_eq!(
        movement, expected,
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
    let mut env = fixtures::env(&world, &carriers, &config);
    let mut actor = TraversalExecutor::new(physics, 3.0);
    let mut body = ActorBody::standing(start);
    actor.set_route(route);
    for _ in 0..5 {
        actor.step(&env, &mut body);
    }
    env.open = &open;
    for _ in 0..90 {
        actor.step(&env, &mut body);
    }
    assert_eq!(actor.status, TraversalStatus::LostSupport);
    assert_eq!(body.support, CharacterSupport::Ground);
    assert!((body.position.y - start.y).abs() < 0.1, "{body:?}");
    env.open = &[];
    for _ in 0..150 {
        actor.step(&env, &mut body);
    }
    assert_eq!(actor.status, TraversalStatus::Reached, "{actor:?}");
    assert!(body.position.distance_sq(&goal) < 0.1);
}
