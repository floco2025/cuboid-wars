use super::*;
// Test copy of the default `player.jump_speed` config value.
const TEST_JUMP_SPEED: f32 = 12.0;

#[test]
fn supported_player_can_start_jump() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };

    assert_eq!(
        player_jump_velocity(0.0, &collision_world, player_physics(), TEST_JUMP_SPEED, &pos, &[]),
        Some(TEST_JUMP_SPEED)
    );
}

#[test]
fn airborne_player_cannot_start_jump() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 1.0, z: 0.0 };

    assert_eq!(
        player_jump_velocity(0.0, &collision_world, player_physics(), TEST_JUMP_SPEED, &pos, &[]),
        None
    );
}

#[test]
fn upward_jump_velocity_moves_player_above_support() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
    let motion = player_jump_velocity(0.0, &collision_world, player_physics(), TEST_JUMP_SPEED, &pos, &[])
        .expect("supported player should start a jump");

    let step = step_character_movement(
        character_step_toward(pos, motion, pos.x, pos.z, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            powered_bridges: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
            portals: None,
        },
    );

    assert!(step.position.y > pos.y);
    assert!(step.vertical_velocity > 0.0);
    assert_eq!(step.support, CharacterSupport::Airborne);
}

#[test]
fn landing_reports_ground_support() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 0.4, z: 0.0 };

    let step = step_character_movement(
        character_step_toward(pos, -10.0, pos.x, pos.z, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            powered_bridges: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
            portals: None,
        },
    );

    assert_eq!(step.vertical_velocity, 0.0);
    assert_eq!(step.support, CharacterSupport::Ground);
}

#[test]
fn upward_motion_hits_floor_underside() {
    let floor = upper_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 1.8, z: 0.0 };
    let motion = TEST_JUMP_SPEED;

    let step = step_character_movement(
        character_step_toward(pos, motion, pos.x, pos.z, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            powered_bridges: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
            portals: None,
        },
    );

    assert_eq!(step.vertical_velocity, 0.0);
    assert!(step.position.y <= floor.y - floor.thickness);
}

#[test]
fn initial_ceiling_contact_does_not_cancel_horizontal_movement() {
    let floor = lower_floor();
    let ceiling = low_overhead_floor();
    let collision_world = collision_world(&[floor, ceiling], &[]);
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
    let motion = 0.0;

    let step = step_character_movement(
        character_step_toward(pos, motion, 0.5, pos.z, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            powered_bridges: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
            portals: None,
        },
    );

    assert!(!step.blocked);
    assert!(step.position.x > pos.x);
    assert!((step.position.y - floor.y).abs() < 0.001);
    assert_eq!(step.vertical_velocity, 0.0);
}

#[test]
fn upward_motion_ignores_floor_underside_outside_footprint() {
    let floor = upper_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 5.0, y: 1.8, z: 0.0 };
    let motion = TEST_JUMP_SPEED;

    let step = step_character_movement(
        character_step_toward(pos, motion, pos.x, pos.z, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            powered_bridges: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
            portals: None,
        },
    );

    assert!(step.vertical_velocity > 0.0);
    assert!(step.position.y > pos.y);
}

#[test]
fn upward_motion_under_floor_edge_hits_floor_side() {
    let floor = upper_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position {
        x: -5.0,
        y: 2.3,
        z: 0.0,
    };
    let motion = TEST_JUMP_SPEED;

    let step = step_character_movement(
        character_step_toward(pos, motion, -4.25, pos.z, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            powered_bridges: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
            portals: None,
        },
    );

    assert!(step.blocked);
    assert!(step.position.x > pos.x);
}

#[test]
fn player_on_floor_top_can_move_over_adjacent_floor_slab_edge() {
    let floor = upper_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position {
        x: -5.0,
        y: floor.y,
        z: 0.0,
    };
    let motion = 0.0;

    let step = step_character_movement(
        character_step_toward(pos, motion, -3.75, pos.z, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            powered_bridges: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
            portals: None,
        },
    );

    assert!(!step.blocked);
    assert!(step.position.x > pos.x);
    assert!(
        step.position.y >= floor.y - 0.01,
        "expected player to remain near floor top, got {step:?}"
    );
}
