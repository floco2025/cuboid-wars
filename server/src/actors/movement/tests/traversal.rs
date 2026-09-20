use super::*;
use crate::actors::navigation::surface::{RouteFailure, SurfaceMesh, fixtures};
use common::protocol::FieldId;

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
