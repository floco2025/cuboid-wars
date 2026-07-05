use super::*;
use crate::constants::{PLAYER_JUMP_SPEED, TICK_SECS};

// Center past the floor edge: outside the support probe footprint but inside
// the collider footprint — the band where the probe reads airborne while the
// collider still rests on the edge.
fn edge_overhang_x(floor: &Floor, physics: CharacterPhysicsConfig) -> f32 {
    floor.x2 + f32::midpoint(physics.support_probe.width, physics.collider.width) / 2.0
}

#[test]
fn edge_overhang_does_not_accumulate_fall_velocity() {
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
            &pos,
            vertical_velocity,
            &collision_world,
            false,
            false,
            &[],
            physics,
            pos.x,
            pos.z,
            TICK_SECS,
        );
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
    }

    assert_eq!(vertical_velocity, 0.0);
    // With the probe missing there is no ground-follow, so the character
    // settles the `bottom_y_offset` gap until the collider itself rests on
    // the edge — but it must not fall past that support height.
    assert!(
        pos.y >= floor.y - physics.collider.bottom_y_offset() - 0.05,
        "character fell past the edge support: {pos:?}"
    );
    assert!(pos.y <= floor.y + 0.05, "character rose above the floor top: {pos:?}");
}

#[test]
fn edge_overhang_allows_jump() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let physics = player_physics();
    let pos = Position {
        x: edge_overhang_x(&floor, physics),
        y: floor.y,
        z: 0.0,
    };
    let mut motion = 0.0;

    assert!(try_start_player_jump(
        &mut motion,
        &collision_world,
        physics,
        &pos,
        pos.x,
        pos.z
    ));
    assert_eq!(motion, PLAYER_JUMP_SPEED);
}
