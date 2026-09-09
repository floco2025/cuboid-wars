use super::*;
use crate::{constants::CHARACTER_TERMINAL_VELOCITY, protocol::CarrierId};

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

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, 1.0, pos.z, 0.1),
        LadderMode::Automatic,
    );

    assert!(step.blocked, "{step:?}");
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

    let first = step_in(
        &collision_world,
        character_step_toward(pos, motion, 1.0, pos.z, 0.1),
        LadderMode::Automatic,
    );
    let second = step_in(
        &collision_world,
        character_step_toward(first.position, motion, 1.0, first.position.z, 0.1),
        LadderMode::Automatic,
    );

    assert!(first.blocked, "{first:?}");
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

    let first = step_in(
        &collision_world,
        character_step_toward(pos, motion, 1.0, pos.z, 0.1),
        LadderMode::Automatic,
    );
    let second = step_in(
        &collision_world,
        character_step_toward(first.position, motion, 1.0, first.position.z + 1.0, 0.1),
        LadderMode::Automatic,
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

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, 30.394, 31.699, delta),
        LadderMode::Automatic,
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

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, 1.0, 1.0, 0.1),
        LadderMode::Automatic,
    );

    assert!(step.blocked, "{step:?}");
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
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
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
    let velocity = Vec3::new(1.0, 0.0, 0.25).normalize() * TEST_PLAYER_SPEED;

    for _ in 0..120 {
        let step = step_in(
            &collision_world,
            CharacterStep {
                start: pos,
                vertical_velocity: motion,
                control_velocity: velocity,
                external_displacement: Vec3::ZERO,
                delta,
            },
            LadderMode::Automatic,
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

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, 1.0, 1.0, 0.1),
        LadderMode::Automatic,
    );

    assert!(step.blocked, "{step:?}");
    assert!(step.position.x > pos.x);
    assert!(
        step.position.z < 0.0 || step.position.x < wall.x1 - player_physics().movement_collider.radius(),
        "{step:?}"
    );
}

#[test]
fn jumping_while_pushing_into_a_wall_still_rises() {
    let collision_world = collision_world_with(&[test_wall()], &[lower_floor()], &[]);
    let delta = 1.0 / 30.0;
    let mut pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    for _ in 0..30 {
        pos = step_in(
            &collision_world,
            character_step_toward(pos, 0.0, pos.x + 0.2, pos.z, delta),
            LadderMode::Automatic,
        )
        .position;
    }
    let pressed = step_in(
        &collision_world,
        character_step_toward(pos, 0.0, pos.x + 0.2, pos.z, delta),
        LadderMode::Automatic,
    );
    assert!(pressed.blocked, "the run-up never reached the wall");

    let launch = player_jump_velocity(0.0, &collision_world, player_physics(), 12.0, &pos);
    assert_eq!(
        launch,
        Some(12.0),
        "the jump was refused while pressed into the wall at {pos:?}"
    );

    let mut vertical_velocity = 12.0;
    let mut heights = Vec::new();
    for _ in 0..6 {
        let step = step_in(
            &collision_world,
            character_step_toward(pos, vertical_velocity, pos.x + 0.2, pos.z, delta),
            LadderMode::Automatic,
        );
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
        heights.push(pos.y);
    }
    assert!(
        heights.windows(2).all(|pair| pair[1] > pair[0]) && vertical_velocity > 0.0,
        "a jump into the wall stalled: heights {heights:?}, velocity {vertical_velocity}"
    );
}

// A diagonal run along the wall meets the wall's end with a slanted contact
// normal, which costs more rise per tick than a straight push.
#[test]
fn jumping_while_sliding_diagonally_along_a_wall_keeps_rising() {
    let collision_world = collision_world_with(&[test_wall()], &[lower_floor()], &[]);
    let delta = 1.0 / 30.0;
    let step = |pos: Position, vertical_velocity: f32| {
        step_in(
            &collision_world,
            CharacterStep {
                start: pos,
                vertical_velocity,
                control_velocity: Vec3::new(9.0, 0.0, 9.0),
                external_displacement: Vec3::ZERO,
                delta,
            },
            LadderMode::Automatic,
        )
    };
    let mut pos = Position {
        x: -1.26,
        y: 0.0,
        z: -0.7,
    };
    let mut vertical_velocity = 0.0;
    for _ in 0..3 {
        let result = step(pos, vertical_velocity);
        pos = result.position;
        vertical_velocity = result.vertical_velocity;
    }
    assert_eq!(
        player_jump_velocity(vertical_velocity, &collision_world, player_physics(), 12.0, &pos),
        Some(12.0)
    );

    vertical_velocity = 12.0;
    let mut heights = Vec::new();
    for _ in 0..8 {
        let result = step(pos, vertical_velocity);
        pos = result.position;
        vertical_velocity = result.vertical_velocity;
        heights.push(pos.y);
    }
    assert!(
        heights.windows(2).all(|pair| pair[1] > pair[0]) && vertical_velocity > 0.0,
        "a diagonal jump along the wall stalled: heights {heights:?}, velocity {vertical_velocity}"
    );
}

#[test]
fn running_across_flat_floor_tiles_keeps_its_speed() {
    for height in [0.0, 4.4, 8.8, 26.4] {
        for speed in [6.0, 9.0, 13.5] {
            for perturbation in [0.0, 0.00001] {
                let floors: Vec<_> = (0..30)
                    .map(|index| Floor {
                        x1: -4.0,
                        x2: 4.0,
                        z1: index as f32 * 4.0,
                        z2: (index + 1) as f32 * 4.0,
                        y: height,
                        ..lower_floor()
                    })
                    .collect();
                let world = collision_world(&floors, &[]);
                let mut pos = Position {
                    x: perturbation,
                    y: height + perturbation,
                    z: 1.0,
                };
                let mut vy = 0.0;
                for tick in 0..250 {
                    let result = step_in(
                        &world,
                        CharacterStep {
                            start: pos,
                            vertical_velocity: vy,
                            control_velocity: Vec3::Z * speed,
                            external_displacement: Vec3::ZERO,
                            delta: 1.0 / 30.0,
                        },
                        LadderMode::Automatic,
                    );
                    assert!(
                        result.position.z - pos.z > speed / 30.0 * 0.99,
                        "tick {tick}, from {pos:?}: {result:?}"
                    );
                    assert!((result.position.y - height).abs() < 0.005, "tick {tick}: {result:?}");
                    pos = result.position;
                    vy = result.vertical_velocity;
                }
            }
        }
    }
}
