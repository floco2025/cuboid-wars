use super::*;
use crate::actors::navigation::surface::fixtures;
use common::protocol::Wall;

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
