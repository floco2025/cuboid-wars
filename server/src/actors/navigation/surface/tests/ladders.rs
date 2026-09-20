use super::*;
use crate::actors::{
    TraversalEnvironment, TraversalExecutor, TraversalStatus,
    navigation::surface::{RouteFailure, fixtures},
};
use common::{
    map::Carriers,
    physics::CollisionWorld,
    protocol::{CarrierId, Floor, MapLayout},
};

#[test]
fn surface_routes_climb_and_descend_a_ladder_with_the_real_motor() {
    let config = fixtures::config();
    let physics = config.expect_actor("scuttler").character.physics();
    let floor = |x1, x2, y| Floor {
        x1,
        x2,
        z1: -3.0,
        z2: 3.0,
        y,
        thickness: 0.2,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    let layout = MapLayout {
        floors: vec![floor(0.0, 5.0, 0.0), floor(-5.0, 0.0, 3.0)],
        ladders: vec![Ladder {
            x1: 0.0,
            x2: 0.0,
            z1: -0.6,
            z2: 0.6,
            nx: 1.0,
            nz: 0.0,
            y: 0.0,
            height: 3.0,
            level: 0,
            levels: 2,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let mut mesh = SurfaceMesh::bake(
        &world.collision_meshes().expect("geometry"),
        CarrierId::WORLD,
        physics,
        &[],
    )
    .expect("mesh");
    mesh.add_ladders(&layout.ladders, physics);
    let lower = Position { x: 2.0, y: 0.0, z: 0.0 };
    let upper = Position {
        x: -2.0,
        y: 3.0,
        z: 0.0,
    };
    assert_eq!(
        mesh.route(lower, upper, 0.7).expect_err("body cannot climb").clone(),
        RouteFailure::Disconnected
    );
    let carriers = Carriers::default();
    let env = TraversalEnvironment {
        world: &world,
        carriers: &carriers,
        settings: &config.settings,
        open: &[],
        delta: 1.0 / 30.0,
    };
    for (start, goal) in [(lower, upper), (upper, lower)] {
        let route = mesh.route_for(start, goal, 0.7, 4096, true).expect("ladder route");
        assert!(
            route
                .actions
                .iter()
                .any(|action| matches!(action, TraversalAction::Climb { .. }))
        );
        let mut executor = TraversalExecutor::new(start, physics, 2.0, &env);
        executor.set_route(route);
        let mut climbed = false;
        for _ in 0..600 {
            executor.step(&env);
            climbed |= matches!(executor.movement.support, common::physics::CharacterSupport::Ladder);
            assert!(!world.character_penetrates_solid(&executor.movement.position, physics, &[]));
            if executor.status == TraversalStatus::Reached {
                break;
            }
        }
        assert!(
            climbed
                && executor.status == TraversalStatus::Reached
                && executor.movement.position.distance_sq(&goal) < 0.1,
            "{executor:?}"
        );
    }
}
