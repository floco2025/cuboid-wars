use std::f32::consts::{PI, TAU};

use crate::actors::{
    movement::traversal::{TraversalEnvironment, TraversalExecutor, TraversalStatus},
    navigation::surface::{SurfaceMesh, fixtures},
};
use common::{
    map::Carriers,
    math::angle_delta_radians,
    physics::CollisionWorld,
    protocol::{ActorMoveIntent, CarrierId, Floor, MapLayout, Position},
};

#[test]
fn route_replacement_preserves_turn_rate_and_actual_travel_direction() {
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -20.0,
            x2: 20.0,
            z1: -20.0,
            z2: 20.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
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
            world: &world,
            carriers: &carriers,
            settings: &config.settings,
            open: &[],
            delta: 1.0 / hz as f32,
        };
        let mut actor = TraversalExecutor::new(Position::default(), physics, 8.0, &env);
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
                let start = actor.movement.position;
                let facing = actor.facing;
                actor.set_route(mesh.route(start, goal, 0.3).expect("replan"));
                actor.step(&env);
                let turned = angle_delta_radians(actor.facing, facing).abs();
                assert!(turned <= TAU * env.delta + 1e-5, "unbounded turn {turned} at {hz}Hz");
                let offset = bevy::math::Vec3::from(actor.movement.position) - bevy::math::Vec3::from(start);
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
    use crate::actors::navigation::surface::{SurfaceRoute, TraversalAction};
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -4.0,
            x2: 4.0,
            z1: -1.0,
            z2: 1.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let config = fixtures::config();
    let physics = config.expect_actor("scuttler").character.physics();
    let carriers = Carriers::default();
    let env = TraversalEnvironment {
        world: &world,
        carriers: &carriers,
        settings: &config.settings,
        open: &[],
        delta: 1.0 / 30.0,
    };
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
    let mut actor = TraversalExecutor::new(start, physics, 8.0, &env);
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
        actor.step(&env);
        stopped_at_edge |= actor.status == TraversalStatus::LostSupport;
        assert!(angle_delta_radians(actor.facing, heading).abs() <= TAU * env.delta + 1e-5);
        assert!(
            actor.movement.position.y > -0.05 && actor.movement.position.z <= 1.0,
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
