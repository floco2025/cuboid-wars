use super::*;
// Test copy of the shipped maps' `movement.player.jump_speed`.
const TEST_JUMP_SPEED: f32 = 12.0;

#[test]
fn supported_player_can_start_jump() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };

    assert_eq!(
        player_jump_velocity(0.0, &collision_world, player_physics(), TEST_JUMP_SPEED, &pos),
        Some(TEST_JUMP_SPEED)
    );
}

#[test]
fn airborne_player_cannot_start_jump() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 1.0, z: 0.0 };

    assert_eq!(
        player_jump_velocity(0.0, &collision_world, player_physics(), TEST_JUMP_SPEED, &pos),
        None
    );
}

#[test]
fn upward_jump_velocity_moves_player_above_support() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
    let motion = player_jump_velocity(0.0, &collision_world, player_physics(), TEST_JUMP_SPEED, &pos)
        .expect("supported player should start a jump");

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, pos.x, pos.z, 0.1),
        LadderMode::Automatic,
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

    let step = step_in(
        &collision_world,
        character_step_toward(pos, -10.0, pos.x, pos.z, 0.1),
        LadderMode::Automatic,
    );

    assert_eq!(step.vertical_velocity, 0.0);
    assert_eq!(step.support, CharacterSupport::Ground);
    assert_eq!(step.impact_speed, 12.5);
    let standing = step_in(
        &collision_world,
        character_step_toward(step.position, 0.0, pos.x, pos.z, 0.1),
        LadderMode::Automatic,
    );
    assert_eq!(standing.impact_speed, 0.0);
}

#[test]
fn upward_motion_hits_floor_underside() {
    let floor = upper_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 1.8, z: 0.0 };
    let motion = TEST_JUMP_SPEED;

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, pos.x, pos.z, 0.1),
        LadderMode::Automatic,
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

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, 0.5, pos.z, 0.1),
        LadderMode::Automatic,
    );

    assert!(!step.blocked);
    assert!(step.position.x > pos.x);
    assert!((step.position.y - floor.y).abs() < 0.01, "{step:?}");
    assert_eq!(step.vertical_velocity, 0.0);
}

#[test]
fn upward_motion_ignores_floor_underside_outside_footprint() {
    let floor = upper_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 5.0, y: 1.8, z: 0.0 };
    let motion = TEST_JUMP_SPEED;

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, pos.x, pos.z, 0.1),
        LadderMode::Automatic,
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

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, -4.25, pos.z, 0.1),
        LadderMode::Automatic,
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

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, -3.75, pos.z, 0.1),
        LadderMode::Automatic,
    );

    assert!(!step.blocked);
    assert!(step.position.x > pos.x);
    assert!(
        step.position.y >= floor.y - 0.01,
        "expected player to remain near floor top, got {step:?}"
    );
}

#[test]
fn landing_keeps_incoming_speed_when_a_ground_probe_stops_the_fall() {
    let world = collision_world(&[lower_floor()], &[]);
    let pos = Position::default();
    let result = step_in(
        &world,
        character_step_toward(pos, -15.0, 0.0, 0.0, 0.1),
        LadderMode::Automatic,
    );
    assert_eq!(result.vertical_velocity, 0.0);
    assert_eq!(result.impact_speed, 15.0);
}

#[test]
fn landing_speed_uses_accumulated_velocity_with_only_this_steps_gravity_change() {
    let world = collision_world(&[lower_floor()], &[]);
    let carriers = Carriers::default();
    for gravity in [0.0, 5.0, 25.0] {
        let mut env = test_environment(&world, &carriers, player_physics(), LadderMode::Automatic);
        env.gravity = gravity;
        let pos = Position {
            y: 0.4,
            ..Default::default()
        };
        let result = step_character_movement(character_step_toward(pos, -20.0, 0.0, 0.0, 0.1), &env);
        assert_eq!(result.impact_speed, 20.0 + gravity * 0.1);
        assert_eq!(result.vertical_velocity, 0.0);
    }
}
