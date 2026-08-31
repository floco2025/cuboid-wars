use super::*;
use crate::constants::CHARACTER_TERMINAL_VELOCITY;

#[test]
fn player_hits_wall_collider_from_collision_world() {
    let wall = test_wall();
    let floor = lower_floor();
    let collision_world = collision_world_with(&[wall], &[floor], &[]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let motion = 0.0;

    let step = step_character_movement(
        character_step_toward(pos, motion, 1.0, pos.z, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
        },
    );

    assert!(step.blocked);
    assert!(step.position.x < 0.0);
}

#[test]
fn repeated_wall_pressure_does_not_leak_through_wall() {
    let wall = test_wall();
    let floor = lower_floor();
    let collision_world = collision_world_with(&[wall], &[floor], &[]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let motion = 0.0;

    let first = step_character_movement(
        character_step_toward(pos, motion, 1.0, pos.z, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
        },
    );
    let second = step_character_movement(
        character_step_toward(first.position, motion, 1.0, first.position.z, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
        },
    );

    assert!(first.blocked);
    assert!(second.blocked);
    assert!(second.position.x < 0.0);
}

#[test]
fn player_slides_along_wall_under_pressure() {
    let wall = test_wall();
    let floor = lower_floor();
    let collision_world = collision_world_with(&[wall], &[floor], &[]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let motion = 0.0;

    let first = step_character_movement(
        character_step_toward(pos, motion, 1.0, pos.z, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
        },
    );
    let second = step_character_movement(
        character_step_toward(first.position, motion, 1.0, first.position.z + 1.0, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
        },
    );

    assert!(second.blocked);
    assert!(second.position.x < 0.0);
    assert!(second.position.z > first.position.z);
}

#[test]
fn falling_player_pushing_into_wall_keeps_falling() {
    let wall = upper_horizontal_wall();
    let collision_world = collision_world_with(&[wall], &[], &[]);
    let pos = Position {
        x: 30.391_533,
        y: 7.973_196,
        z: 31.539_902,
    };
    let motion = -CHARACTER_TERMINAL_VELOCITY;
    let delta = 0.0177;

    let step = step_character_movement(
        character_step_toward(pos, motion, 30.394, 31.699, delta),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
        },
    );

    assert!(
        step.position.y < pos.y - 0.9 * CHARACTER_TERMINAL_VELOCITY * delta,
        "expected falling to continue while sliding on wall, got {step:?}"
    );
    assert!(step.vertical_velocity < 0.0);
}

#[test]
fn diagonal_wall_hit_slides_in_same_step() {
    let wall = test_wall();
    let collision_world = collision_world_with(&[wall], &[], &[]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let motion = 0.0;

    let step = step_character_movement(
        character_step_toward(pos, motion, 1.0, 1.0, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
        },
    );

    assert!(step.blocked);
    assert!(step.position.x < 0.0);
    assert!(step.position.z > 0.0);
}

#[test]
fn repeated_diagonal_wall_pressure_keeps_sliding() {
    let wall = Wall {
        x1: 0.0,
        z1: -100.0,
        x2: 0.0,
        z2: 100.0,
        width: WALL_THICKNESS,
        level: 0,
    };
    let floor = lower_floor();
    let collision_world = collision_world_with(&[wall], &[floor], &[]);
    let mut pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let motion = 0.0;
    let delta = 1.0 / 60.0;
    let velocity = Vec3::new(1.0, 0.0, 0.25).normalize() * player_speed();

    for _ in 0..120 {
        let step = step_character_movement(
            CharacterStep {
                start: pos,
                vertical_velocity: motion,
                control_velocity: velocity,
                external_displacement: Vec3::ZERO,
                delta,
            },
            &CharacterEnvironment {
                collision_world: &collision_world,
                gravity: TEST_GRAVITY,
                passable_kinds: &[],
                ladder_climb_ratio: test_ladders(),
                physics: player_physics(),
            },
        );
        pos = step.position;
    }

    assert!(pos.x < 0.0);
    assert!(pos.z > 2.0);
}

#[test]
fn diagonal_wall_end_hit_slides_along_wall() {
    let wall = horizontal_wall();
    let collision_world = collision_world_with(&[wall], &[], &[]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: -1.0,
    };
    let motion = 0.0;

    let step = step_character_movement(
        character_step_toward(pos, motion, 1.0, 1.0, 0.1),
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
        },
    );

    assert!(step.blocked);
    assert!(step.position.x > pos.x);
    assert!(step.position.z < 0.0);
}
