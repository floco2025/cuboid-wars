use super::*;
use crate::{
    constants::MISSILE_RADIUS,
    test_fixtures::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS},
};
use common::protocol::{CarrierId, Floor, MapLayout, Wall};
use std::f32::consts::SQRT_2;

#[test]
fn steer_clamps_rotation_to_the_turn_radius_arc() {
    let velocity = Vec3::Z * 12.0;
    let steered = steer(velocity, Vec3::X, 6.0, 0.1);
    let angle = velocity.angle_between(steered);
    assert!((angle - 0.2).abs() < 1e-4, "expected a 0.2 rad step, got {angle}");
}

#[test]
fn steer_preserves_speed() {
    let velocity = Vec3::new(3.0, 4.0, 12.0);
    let steered = steer(velocity, Vec3::new(-5.0, 0.2, 1.0), 3.0, 0.033);
    assert!((steered.length() - velocity.length()).abs() < 1e-3);
}

#[test]
fn steer_aligns_when_within_one_step() {
    let velocity = Vec3::Z * 10.0;
    let objective = Vec3::new(0.05, 0.0, 1.0);
    let steered = steer(velocity, objective, 7.0, 0.1);
    assert!(steered.normalize().dot(objective.normalize()) > 0.9999);
}

#[test]
fn steer_without_objective_keeps_velocity() {
    let velocity = Vec3::Z * 10.0;
    assert_eq!(steer(velocity, Vec3::ZERO, 7.0, 0.1), velocity);
}

#[test]
fn closest_point_on_segment_finds_the_nearest_pass() {
    let start = Vec3::new(0.0, 0.0, -2.0);
    let travel = Vec3::new(0.0, 0.0, 4.0);
    // Target abeam of the segment's midpoint: closest pass is at z=1.
    let target = Vec3::new(1.0, 0.0, 1.0);

    let closest = closest_point_on_segment(start, travel, target);

    assert!((closest - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-5);
    // Past the segment end the clamp holds.
    let beyond = closest_point_on_segment(start, travel, Vec3::new(0.0, 0.0, 10.0));
    assert_eq!(beyond, Vec3::new(0.0, 0.0, 2.0));
}

#[test]
fn weave_zero_strength_is_straight() {
    let to_target = Vec3::new(3.0, 1.0, 20.0);
    assert_eq!(weave_direction(to_target, 1.234, 0.7, 0.0), to_target);
}

#[test]
fn weave_bends_within_bounds_and_fades_when_close() {
    let far = Vec3::Z * 30.0;
    let bent = weave_direction(far, 0.4, 1.0, 0.35);
    let angle = far.angle_between(bent);
    assert!(angle > 0.0, "far from the target the path wobbles");
    // Max deviation: |wobble| <= strength * sqrt(2).
    assert!(angle <= (0.35_f32 * SQRT_2).atan() + 1e-3);
    assert!((bent.length() - far.length()).abs() < 1e-3, "range is preserved");

    let near = Vec3::Z * 0.5;
    let near_bent = weave_direction(near, 0.4, 1.0, 0.35);
    assert!(
        near.angle_between(near_bent) < 0.35 * (0.5 / WEAVE_FADE_DISTANCE) * SQRT_2 + 1e-3,
        "the wobble fades on final approach"
    );
}

#[test]
fn lead_point_aims_ahead_of_a_moving_target() {
    let origin = Vec3::ZERO;
    let target = Vec3::new(0.0, 0.0, 12.0);
    let velocity = Vec3::new(4.0, 0.0, 0.0);

    let point = lead_point(origin, target, velocity, 12.0);

    // 12 m away at 12 m/s → 1 s of lead → 4 m ahead along the retreat.
    assert!((point - Vec3::new(4.0, 0.0, 12.0)).length() < 1e-4);
}

#[test]
fn lead_point_static_target_is_the_target() {
    let target = Vec3::new(3.0, 1.0, 7.0);
    assert_eq!(lead_point(Vec3::ZERO, target, Vec3::ZERO, 12.0), target);
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
    let layout = MapLayout {
        walls: vec![Wall {
            x1: -20.0,
            z1: 2.0,
            x2: 20.0,
            z2: 2.0,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let origin = Vec3::new(0.0, 2.0, 0.0);

    let picked =
        pick_clear_direction(&world, &[], origin, Vec3::Z, 7.2, 0.3).expect("an upward candidate should be clear");

    assert!(picked.y > 0.5, "expected a climbing direction, got {picked}");
}

#[test]
fn pick_clear_direction_in_open_space_returns_desired() {
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let picked = pick_clear_direction(&world, &[], Vec3::new(0.0, 5.0, 0.0), Vec3::Z, 7.2, 0.3)
        .expect("open space always has a clear candidate");
    assert!(
        picked.angle_between(Vec3::Z).to_degrees() < 1.0,
        "nothing blocked: fly at the target"
    );
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
    let world = CollisionWorld::from_map_layout(&MapLayout {
        walls: vec![Wall {
            x1: 0.0,
            z1: -4.0,
            x2: 0.0,
            z2: 4.0,
            width: WALL_THICKNESS,
            y: 0.0,
            height: WALL_HEIGHT,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
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
