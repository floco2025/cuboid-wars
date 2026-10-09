use super::*;
use crate::actors::{
    ActorBody, TraversalEnvironment, TraversalExecutor, TraversalStatus, navigation::surface::fixtures,
};
use bevy::math::Vec3;
use common::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    math::angle_delta_radians,
    physics::CollisionWorld,
    protocol::{CarrierId, MapLayout, Wall},
};

fn obstacle_scene() -> (SurfaceMesh, CollisionWorld) {
    let layout = MapLayout {
        floors: vec![fixtures::floor([-12.0, 12.0], [-8.0, 8.0], 0.0)],
        walls: vec![Wall {
            x1: 0.0,
            x2: 0.0,
            z1: -1.0,
            z2: 1.0,
            width: 2.0,
            y: 0.0,
            height: 3.0,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let mesh = SurfaceMesh::bake(
        &world.collision_meshes().expect("collision mesh"),
        CarrierId::WORLD,
        scuttler(),
        &[],
    )
    .expect("walkable surface");
    (mesh, world)
}

fn scuttler() -> CharacterPhysicsConfig {
    fixtures::config().expect_actor("scuttler").character.physics()
}

// Walks `route` from `start` with the real motor, never into a solid, and
// asserts it arrives at `goal`.
fn assert_walks(
    env: &TraversalEnvironment,
    physics: CharacterPhysicsConfig,
    route: SurfaceRoute,
    start: Position,
    goal: Position,
) {
    let mut actor = TraversalExecutor::new(physics, 3.0);
    let mut body = ActorBody::standing(start);
    actor.set_route(route);
    for _ in 0..1200 {
        actor.step(env, &mut body);
        assert!(
            !env.world.character_penetrates_solid(&body.position, physics, &[]),
            "{actor:?}"
        );
        if actor.status == TraversalStatus::Reached {
            break;
        }
    }
    assert_eq!(actor.status, TraversalStatus::Reached, "{actor:?}");
    assert!(body.position.distance_sq(&goal) < 0.1, "{actor:?}");
}

#[test]
fn open_pursuit_paths_stay_straight_across_polygon_boundaries() {
    let (mesh, _) = obstacle_scene();
    for z in [-3.0, 3.0] {
        let start = Position { x: -8.0, y: 0.0, z };
        let goal = Position { x: 8.0, y: 0.0, z };
        let route = mesh.route(start, goal, 0.3).expect("open route");
        let mut previous = start;
        for action in &route.actions {
            let TraversalAction::Walk { target, .. } = action else {
                panic!("unexpected traversal");
            };
            assert!(
                (target.z - z).abs() < 0.02 && target.x >= previous.x - 0.01,
                "unnecessary sideways/backward waypoint: {action:?}; {:?}",
                route.actions
            );
            previous = *target;
        }
    }
}

#[test]
fn smoothed_routes_go_around_obstacles_without_cutting_clearance() {
    let (mesh, world) = obstacle_scene();
    let physics = scuttler();
    for direction in [-1.0, 1.0] {
        let start = Position {
            x: -8.0 * direction,
            y: 0.0,
            z: 0.0,
        };
        let goal = Position {
            x: 8.0 * direction,
            y: 0.0,
            z: 0.0,
        };
        let route = mesh.route(start, goal, 0.3).expect("around the obstacle");
        let mut previous = start;
        let mut length = 0.0;
        for action in &route.actions {
            let TraversalAction::Walk { target, .. } = action else {
                panic!("unexpected traversal");
            };
            length += previous.horizontal_distance_sq(target).sqrt();
            for step in 0..=100 {
                let point = Vec3::from(previous).lerp((*target).into(), step as f32 / 100.0);
                assert!(
                    mesh.locate(point.into(), 0.12).is_some(),
                    "left walkable clearance at {point:?}"
                );
                assert!(
                    !world.character_penetrates_solid(&point.into(), physics, &[]),
                    "cut into obstacle at {point:?}"
                );
            }
            previous = *target;
        }
        assert!(length < 18.0, "unnecessary detour ({length}m): {:?}", route.actions);
    }
}

#[test]
fn smoothing_preserves_the_ramp_route_between_stacked_surfaces() {
    let config = fixtures::config();
    let generated = fixtures::generate("fixture", 30, &config.settings).expect("ramp scene");
    let world = CollisionWorld::from_map_layout(&generated.layout);
    let carriers = Carriers::from_layout(&generated.layout);
    let physics = config.expect_actor("scuttler").character.physics();
    let mesh = SurfaceMesh::bake(
        &world.collision_meshes().expect("collision"),
        CarrierId::WORLD,
        physics,
        &[],
    )
    .expect("mesh");
    let start = Position { x: 4.5, y: 0.0, z: 0.0 };
    let goal = Position { y: 3.0, ..start };
    let route = mesh.route(start, goal, 0.3).expect("ramp route");
    assert!(
        route
            .actions
            .iter()
            .any(|action| matches!(action, TraversalAction::Walk { target, .. } if target.x < -8.0)),
        "shortcut between stacked floors: {:?}",
        route.actions
    );
    assert_walks(&fixtures::env(&world, &carriers, &config), physics, route, start, goal);
}

#[test]
fn smoothed_obstacle_routes_remain_executable_with_bounded_turning() {
    let (mesh, world) = obstacle_scene();
    let config = fixtures::config();
    let physics = config.expect_actor("scuttler").character.physics();
    let carriers = Carriers::default();
    let env = fixtures::env(&world, &carriers, &config);
    for direction in [-1.0, 1.0] {
        let start = Position {
            x: -8.0 * direction,
            y: 0.0,
            z: 0.0,
        };
        let goal = Position {
            x: 8.0 * direction,
            y: 0.0,
            z: 0.0,
        };
        let mut actor = TraversalExecutor::new(physics, 8.0);
        let mut body = ActorBody::standing(start);
        actor.set_route(mesh.route(start, goal, 0.3).expect("obstacle route"));
        for _ in 0..300 {
            let previous = actor.facing;
            actor.step(&env, &mut body);
            assert!(angle_delta_radians(actor.facing, previous).abs() <= std::f32::consts::TAU * env.delta + 1e-5);
            assert!(
                !world.character_penetrates_solid(&body.position, physics, &[]),
                "{actor:?}"
            );
            if actor.status == TraversalStatus::Reached {
                break;
            }
        }
        assert_eq!(actor.status, TraversalStatus::Reached, "{actor:?}");
        assert!(body.position.distance_sq(&goal) < 0.1, "{actor:?}");
    }
}

#[test]
fn exhausting_the_smoothing_budget_keeps_the_found_route_executable() {
    let (mesh, world) = obstacle_scene();
    let start = Position {
        x: -8.0,
        y: 0.0,
        z: 0.0,
    };
    let goal = Position { x: 8.0, y: 0.0, z: 0.0 };
    let full = mesh.route(start, goal, 0.3).expect("smoothed route");
    let limit = full.expanded - 1;
    let route = mesh
        .route_with_limit(start, goal, 0.3, limit)
        .expect("existing route survives incomplete smoothing");
    assert_eq!(route.expanded, limit);
    let config = fixtures::config();
    let carriers = Carriers::default();
    assert_walks(
        &fixtures::env(&world, &carriers, &config),
        scuttler(),
        route,
        start,
        goal,
    );
}
