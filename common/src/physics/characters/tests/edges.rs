use super::*;
use crate::constants::TICK_SECS;

// Center past the floor edge: outside the support probe footprint but inside
// the collider footprint — the band where the probe reads airborne while the
// collider still rests on the edge ("perched").
fn edge_overhang_x(floor: &Floor, physics: CharacterPhysicsConfig) -> f32 {
    floor.x2 + f32::midpoint(physics.support_probe.width, physics.collider.width) / 2.0
}

#[test]
fn edge_overhang_slides_off_and_falls() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let physics = player_physics();
    let mut pos = Position {
        x: edge_overhang_x(&floor, physics),
        y: floor.y,
        z: 0.0,
    };
    let mut vertical_velocity = 0.0;

    for _ in 0..30 {
        let step = step_character_movement(
            character_step_toward(pos, vertical_velocity, pos.x, pos.z, TICK_SECS),
            &CharacterEnvironment {
                collision_world: &collision_world,
                gravity: TEST_GRAVITY,
                passable_kinds: &[],
                powered_bridges: &[],
                ladder_climb_ratio: test_ladders(),
                physics,
                portals: None,
            },
        );
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
    }

    assert!(
        pos.x > floor.x2 + physics.collider.width / 2.0,
        "expected the slide to clear the edge, got {pos:?}"
    );
    assert!(vertical_velocity < 0.0);
    assert!(pos.y < floor.y - 1.0, "expected a real fall, got {pos:?}");
}

#[test]
fn edge_overhang_denies_jump() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let physics = player_physics();
    let pos = Position {
        x: edge_overhang_x(&floor, physics),
        y: floor.y,
        z: 0.0,
    };
    assert_eq!(
        player_jump_velocity(0.0, &collision_world, physics, 12.0, &pos, &[]),
        None
    );
}

#[test]
fn probe_grounded_near_edge_does_not_slide() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let physics = player_physics();
    let mut pos = Position {
        x: floor.x2,
        y: floor.y,
        z: 0.0,
    };
    let mut vertical_velocity = 0.0;

    for _ in 0..5 {
        let step = step_character_movement(
            character_step_toward(pos, vertical_velocity, pos.x, pos.z, TICK_SECS),
            &CharacterEnvironment {
                collision_world: &collision_world,
                gravity: TEST_GRAVITY,
                passable_kinds: &[],
                powered_bridges: &[],
                ladder_climb_ratio: test_ladders(),
                physics,
                portals: None,
            },
        );
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
    }

    assert!((pos.x - floor.x2).abs() < 1e-3, "grounded character drifted: {pos:?}");
    assert!((pos.y - floor.y).abs() < 0.01);
    assert_eq!(vertical_velocity, 0.0);
}

#[test]
fn input_overrides_perch_slide() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let physics = player_physics();
    let run_speed = player_speed();
    let mut pos = Position {
        x: edge_overhang_x(&floor, physics),
        y: floor.y,
        z: 0.0,
    };
    let mut vertical_velocity = 0.0;

    for _ in 0..15 {
        let step = step_character_movement(
            character_step_toward(
                pos,
                vertical_velocity,
                run_speed.mul_add(-TICK_SECS, pos.x),
                pos.z,
                TICK_SECS,
            ),
            &CharacterEnvironment {
                collision_world: &collision_world,
                gravity: TEST_GRAVITY,
                passable_kinds: &[],
                powered_bridges: &[],
                ladder_climb_ratio: test_ladders(),
                physics,
                portals: None,
            },
        );
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
    }

    assert!(
        position_has_floor_support(&collision_world, &pos, physics, &[]),
        "expected the walk-back to regain probe support: {pos:?}"
    );
    assert!((pos.y - floor.y).abs() < 0.05);
    assert_eq!(vertical_velocity, 0.0);
}

#[test]
fn blocked_slide_keeps_velocity_zeroed() {
    let floor = lower_floor();
    // Long wall just past the edge: the slide direction is blocked, so the
    // perch persists — the safety net must keep fall velocity from pumping,
    // and the idle player must not read as blocked (bump feedback).
    let wall = Wall {
        x1: 5.0,
        z1: -100.0,
        x2: 5.0,
        z2: 100.0,
        width: 0.2,
        level: 0,
    };
    let collision_world = collision_world_with(&[wall], &[floor], &[]);
    let physics = player_physics();
    let mut pos = Position {
        x: edge_overhang_x(&floor, physics),
        y: floor.y,
        z: 0.0,
    };
    let mut vertical_velocity = 0.0;
    let mut last_blocked = false;

    for _ in 0..30 {
        let step = step_character_movement(
            character_step_toward(pos, vertical_velocity, pos.x, pos.z, TICK_SECS),
            &CharacterEnvironment {
                collision_world: &collision_world,
                gravity: TEST_GRAVITY,
                passable_kinds: &[],
                powered_bridges: &[],
                ladder_climb_ratio: test_ladders(),
                physics,
                portals: None,
            },
        );
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
        last_blocked = step.blocked;
        // Bounded by the free fall across the `bottom_y_offset` gap — never
        // the runaway accumulation of the phantom-fall bug.
        assert!(vertical_velocity > -6.0, "fall velocity pumped: {vertical_velocity}");
    }

    assert_eq!(vertical_velocity, 0.0);
    assert!(
        pos.x < floor.x2 + physics.collider.width / 2.0,
        "escaped the wall: {pos:?}"
    );
    assert!(pos.y >= floor.y - physics.collider.bottom_y_offset() - 0.05);
    assert!(!last_blocked, "idle perched character reported blocked");
}
