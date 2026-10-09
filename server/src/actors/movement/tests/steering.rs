use std::f32::consts::{PI, TAU};

use crate::actors::{
    movement::traversal::{ActorBody, TraversalEnvironment, TraversalExecutor, TraversalStatus},
    navigation::surface::{SurfaceMesh, SurfaceRoute, TraversalAction, fixtures},
};
use bevy::math::Vec3;
use common::{
    map::Carriers,
    math::angle_delta_radians,
    physics::CollisionWorld,
    protocol::{ActorMoveIntent, CarrierId, MapLayout, Position},
};

#[test]
fn route_replacement_preserves_turn_rate_and_actual_travel_direction() {
    let world = CollisionWorld::from_map_layout(&MapLayout {
        floors: vec![fixtures::floor([-20.0, 20.0], [-20.0, 20.0], 0.0)],
        ..Default::default()
    });
    let config = fixtures::config();
    let physics = config.expect_actor("scuttler").character.physics();
    let mesh = SurfaceMesh::bake(
        &world.collision_meshes().expect("collision"),
        CarrierId::WORLD,
        physics,
        &[],
    )
    .expect("mesh");
    let carriers = Carriers::default();
    for hz in [30, 60, 120] {
        let env = TraversalEnvironment {
            delta: 1.0 / hz as f32,
            ..fixtures::env(&world, &carriers, &config)
        };
        let mut actor = TraversalExecutor::new(physics, 8.0);
        let mut body = ActorBody::standing(Position::default());
        // Crossing the -pi/pi boundary must take the short turn as well.
        for goal in [
            Position {
                x: 0.0,
                y: 0.0,
                z: -8.0,
            },
            Position {
                x: -1.0,
                y: 0.0,
                z: -16.0,
            },
            Position::default(),
        ] {
            let mut reached = false;
            for _ in 0..hz * 5 {
                let start = body.position;
                let facing = actor.facing;
                actor.set_route(mesh.route(start, goal, 0.3).expect("replan"));
                actor.step(&env, &mut body);
                let turned = angle_delta_radians(actor.facing, facing).abs();
                assert!(turned <= TAU * env.delta + 1e-5, "unbounded turn {turned} at {hz}Hz");
                let offset = Vec3::from(body.position) - Vec3::from(start);
                if offset.x.hypot(offset.z) > 1e-4 {
                    assert!(
                        angle_delta_radians(offset.x.atan2(offset.z), actor.facing).abs() < 0.01,
                        "travel and facing diverged"
                    );
                }
                let desired = (goal.x - start.x).atan2(goal.z - start.z);
                if angle_delta_radians(desired, actor.facing).abs() > PI / 2.0 {
                    assert!(
                        offset.x.hypot(offset.z) < 1e-4,
                        "must pivot before walking toward a target behind us"
                    );
                    assert!(matches!(actor.intent, ActorMoveIntent::Moving { speed: 0.0, .. }));
                }
                if actor.status == TraversalStatus::Reached {
                    reached = true;
                    break;
                }
            }
            assert!(reached, "{hz}Hz: {actor:?}");
        }
    }
}

#[test]
fn actor_facing_off_a_ledge_can_turn_back_onto_safe_ground() {
    let world = CollisionWorld::from_map_layout(&MapLayout {
        floors: vec![fixtures::floor([-4.0, 4.0], [-1.0, 1.0], 0.0)],
        ..Default::default()
    });
    let config = fixtures::config();
    let physics = config.expect_actor("scuttler").character.physics();
    let carriers = Carriers::default();
    let env = fixtures::env(&world, &carriers, &config);
    let start = Position {
        x: 0.0,
        y: 0.0,
        z: 0.99,
    };
    let goal = Position {
        x: -3.0,
        y: 0.0,
        z: 0.0,
    };
    let mut actor = TraversalExecutor::new(physics, 8.0);
    let mut body = ActorBody::standing(start);
    actor.set_route(SurfaceRoute {
        actions: [TraversalAction::Walk {
            carrier: CarrierId::WORLD,
            target: goal,
        }]
        .into(),
        expanded: 0,
    });
    let mut stopped_at_edge = false;
    for _ in 0..120 {
        let heading = actor.facing;
        actor.step(&env, &mut body);
        stopped_at_edge |= actor.status == TraversalStatus::LostSupport;
        assert!(angle_delta_radians(actor.facing, heading).abs() <= TAU * env.delta + 1e-5);
        assert!(
            body.position.y > -0.05 && body.position.z <= 1.0,
            "stepped off the ledge: {actor:?}"
        );
        if actor.status == TraversalStatus::Reached {
            break;
        }
    }
    assert!(stopped_at_edge, "fixture must exercise the support guard");
    assert_eq!(
        actor.status,
        TraversalStatus::Reached,
        "failed to turn toward safe ground: {actor:?}"
    );
}
