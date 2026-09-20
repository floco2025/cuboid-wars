use super::*;
use crate::actors::navigation::surface::fixtures;
use common::protocol::{Floor, Wall};

#[test]
fn returning_from_exterior_enters_coverage_before_routing_under_a_roof() {
    let config = fixtures::config();
    let mut generated = fixtures::compile(serde_json::json!({"map": {
        "grid_cols":30,"grid_rows":4,"fireworks":null,
        "levels":[{"floors":[{"col":1,"row":1,"all":"basement-floor"}]}],
        "checkpoints":[{"level":0,"cols":[1,2],"rows":[1,2],"number":0,"type":"individual"}],
        "actor_spawn_zones":[{"level":0,"cols":[1,2],"rows":[1,2],"kind":"bruiser","count":[1],"respawn_secs":null,"roam_distance":0}]
    }}), 30, &config.settings).expect("covered destination");
    let floor = |x1, x2, y| Floor {
        x1,
        x2,
        z1: -6.0,
        z2: 6.0,
        y,
        thickness: 0.2,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    generated.layout.floors = vec![floor(-100.0, 100.0, 0.0), floor(-10.0, 10.0, 10.0)];
    let world = CollisionWorld::from_map_layout(&generated.layout);
    let carriers = Carriers::from_layout(&generated.layout);
    let navigation =
        SurfaceNavigation::build(&generated.config, &generated.layout, &config, &world, &[], &[]).expect("navigation");
    let physics = config.expect_actor("bruiser").character.physics();
    for direction in [-1.0, 1.0] {
        let from = SurfaceGoal {
            carrier: CarrierId::WORLD,
            position: Position {
                x: direction * 52.0,
                y: 0.0,
                z: 0.0,
            },
        };
        let to = SurfaceGoal {
            carrier: CarrierId::WORLD,
            position: Position {
                x: direction * -42.0,
                y: 0.0,
                z: 0.0,
            },
        };
        let goal = navigation.approach_goal(from, to, physics, false, &world, &carriers, &[]);
        assert!(goal.position.y.abs() < 0.2, "return leg targeted the roof: {goal:?}");
        assert!(
            goal.position.x * direction > 40.0,
            "return leg skipped entry into loaded coverage: {goal:?}"
        );
        assert!(navigation.can_route(goal, to, physics, false));
        assert_eq!(
            navigation.approach_goal(goal, to, physics, false, &world, &carriers, &[]),
            to
        );
    }
}

#[test]
fn a_target_in_a_corner_is_pursued_at_the_nearest_point_a_wide_body_can_stand() {
    let mut config = fixtures::config();
    let body = &mut config
        .actors
        .get_mut("bruiser")
        .expect("fixture actor")
        .character
        .character;
    body.movement_collider.diameter = 1.64;
    let mut generated = fixtures::compile(
        serde_json::json!({"map": {
            "grid_cols":4,"grid_rows":4,"fireworks":null,
            "levels":[{"floors":(0..4).flat_map(|row| (0..4).map(move |col|
                serde_json::json!({"col":col,"row":row,"all":"basement-floor"}))).collect::<Vec<_>>()}],
            "checkpoints":[{"level":0,"cols":[2,3],"rows":[2,3],"number":0,"type":"individual"}],
            "actor_spawn_zones":[{"level":0,"cols":[2,3],"rows":[2,3],"kind":"bruiser","count":[1],"respawn_secs":null}]
        }}),
        30,
        &config.settings,
    )
    .expect("walled room");
    let wall = |x1, z1, x2, z2| Wall {
        x1,
        z1,
        x2,
        z2,
        width: 0.3,
        y: 0.0,
        height: 3.0,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    generated
        .layout
        .walls
        .extend([wall(-6.0, -6.0, -6.0, 6.0), wall(-6.0, -6.0, 6.0, -6.0)]);
    let world = CollisionWorld::from_map_layout(&generated.layout);
    let navigation =
        SurfaceNavigation::build(&generated.config, &generated.layout, &config, &world, &[], &[]).expect("navigation");
    let physics = config.expect_actor("bruiser").character.physics();
    let from = SurfaceGoal {
        carrier: CarrierId::WORLD,
        position: Position { x: 3.0, y: 0.0, z: 3.0 },
    };
    // A player's 0.3 m capsule pressed into the corner of the two walls.
    let target = SurfaceGoal {
        carrier: CarrierId::WORLD,
        position: Position {
            x: -5.54,
            y: 0.0,
            z: -5.54,
        },
    };
    assert!(
        !navigation.can_route(from, target, physics, false),
        "the corner itself is outside a wide body's mesh"
    );
    let goal = navigation.pursuit_goal(target, physics);
    assert!(navigation.can_route(from, goal, physics, false), "{goal:?}");
    assert!(
        goal.position.distance_sq(&target.position).sqrt() <= physics.movement_collider.radius() * SQRT_2,
        "{goal:?}"
    );
}
