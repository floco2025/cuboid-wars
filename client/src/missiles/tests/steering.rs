use super::super::fixtures::{wall, world};
use super::*;
use crate::{
    constants::MISSILE_RADIUS,
    test_fixtures::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_THICKNESS},
};
use common::protocol::{CarrierId, Floor, MapLayout};

#[test]
fn steer_turns_at_most_the_turn_radius_arc_keeping_the_speed() {
    let velocity = Vec3::Z * 12.0;
    let steered = steer(velocity, Vec3::X, 6.0, 0.1);
    let angle = velocity.angle_between(steered);
    assert!((angle - 0.2).abs() < 1e-4, "expected a 0.2 rad step, got {angle}");
    let tilted = Vec3::new(3.0, 4.0, 12.0);
    let turned = steer(tilted, Vec3::new(-5.0, 0.2, 1.0), 3.0, 0.033);
    assert!((turned.length() - tilted.length()).abs() < 1e-3);
    // Within one step of the objective it aligns exactly.
    let objective = Vec3::new(0.05, 0.0, 1.0);
    let aligned = steer(Vec3::Z * 10.0, objective, 7.0, 0.1);
    assert!(aligned.normalize().dot(objective.normalize()) > 0.9999);
}

#[test]
fn target_velocity_estimate_ignores_teleports() {
    let last = Some(Vec3::ZERO);
    let walked = target_velocity_estimate(last, Vec3::new(0.0, 0.0, 0.132), 0.033);
    assert!((walked.z - 4.0).abs() < 0.01, "normal motion is estimated");

    let jumped = target_velocity_estimate(last, Vec3::new(40.0, 0.0, 0.0), 0.033);
    assert_eq!(jumped, Vec3::ZERO, "a respawn jump reads as stationary");
}

#[test]
fn pick_clear_direction_prefers_climbing_over_a_wall() {
    // A wide wall dead ahead: the 35° climb still clips it within the
    // lookahead, the 70° climb passes above — the pick must go up, not
    // sideways.
    let world = world(&MapLayout {
        walls: vec![wall(-20.0, 2.0, 20.0, 2.0)],
        ..Default::default()
    });
    let origin = Vec3::new(0.0, 2.0, 0.0);

    let picked =
        pick_clear_direction(&world, &[], origin, Vec3::Z, 7.2, 0.3).expect("an upward candidate should be clear");

    assert!(picked.y > 0.5, "expected a climbing direction, got {picked}");
}

#[test]
fn pick_clear_direction_dives_toward_a_target_below() {
    // A floor slab between the missile (above) and its target (below),
    // open past z = 2: the pick must descend toward the opening, not
    // cruise on the upper level.
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -10.0,
            z1: -2.0,
            x2: 10.0,
            z2: 2.0,
            y: LEVEL_HEIGHT,
            thickness: FLOOR_THICKNESS,
            level: 1,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    // Straight down onto the slab is blocked; the fan must find the
    // descending direction past the slab edge.
    let picked = pick_clear_direction(&world, &[], Vec3::new(0.0, 6.0, 0.0), Vec3::NEG_Y, 7.2, 0.3)
        .expect("a descending candidate past the slab edge should be clear");

    assert!(picked.y < -0.2, "expected a diving direction, got {picked}");
}

#[test]
fn a_terminal_approach_starts_from_a_missile_already_touching_geometry() {
    let world = world(&MapLayout {
        walls: vec![wall(0.0, -4.0, 0.0, 4.0)],
        ..Default::default()
    });
    // Skimming the wall face: the start overlaps, the travel away from it does not.
    let origin = Vec3::new(WALL_THICKNESS / 2.0 + 0.2, 1.0, 0.0);
    let target = Vec3::new(6.0, 1.0, 0.0);
    assert!(!sweep_clear(&world, &[], origin, target - origin, MISSILE_RADIUS));
    assert!(travel_clear(&world, &[], origin, target - origin, MISSILE_RADIUS));
    assert_eq!(
        terminal_approach(&world, &[], origin, target, MISSILE_RADIUS, 1.0),
        Some(target)
    );
}
