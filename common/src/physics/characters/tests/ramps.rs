use super::*;

#[test]
fn player_walking_off_ramp_side_is_not_blocked_by_ramp_side() {
    let ramp = test_ramp();
    let collision_world = collision_world(&[], &[ramp]);
    let pos = Position {
        x: 2.0,
        y: ramp_surface_at(&ramp, 2.0, 4.0),
        z: 4.0,
    };
    let motion = 0.0;

    let step = step_character_movement(
        CharacterStep {
            start: pos,
            vertical_velocity: motion,
            target_x: -1.0,
            target_z: pos.z,
            delta: 0.1,
        },
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladders: test_ladders(),
            physics: player_physics(),
        },
    );

    assert!(!step.blocked);
    assert!(step.position.x < pos.x);
}

#[test]
fn lower_floor_player_hits_wedge_side_from_collision_world() {
    let ramp = test_ramp();
    let collision_world = collision_world(&[], &[ramp]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 4.0,
    };
    let motion = 0.0;

    let step = step_character_movement(
        CharacterStep {
            start: pos,
            vertical_velocity: motion,
            target_x: 1.0,
            target_z: pos.z,
            delta: 0.1,
        },
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladders: test_ladders(),
            physics: player_physics(),
        },
    );

    assert!(step.blocked);
    assert!(step.position.x < 0.0);
}

#[test]
fn lower_floor_player_can_enter_wedge_low_end() {
    let ramp = test_ramp();
    let collision_world = collision_world(&[], &[ramp]);
    let pos = Position {
        x: 2.0,
        y: 0.0,
        z: -0.25,
    };
    let motion = 0.0;

    let step = step_character_movement(
        CharacterStep {
            start: pos,
            vertical_velocity: motion,
            target_x: pos.x,
            target_z: 0.25,
            delta: 0.1,
        },
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladders: test_ladders(),
            physics: player_physics(),
        },
    );

    assert!(!step.blocked);
    assert!(step.position.z > pos.z);
}

#[test]
fn upper_floor_player_can_enter_wedge_high_end() {
    let ramp = test_ramp();
    let collision_world = collision_world(&[], &[ramp]);
    let pos = Position {
        x: 2.0,
        y: LEVEL_HEIGHT,
        z: 8.25,
    };
    let motion = 0.0;

    let step = step_character_movement(
        CharacterStep {
            start: pos,
            vertical_velocity: motion,
            target_x: pos.x,
            target_z: 7.75,
            delta: 0.1,
        },
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladders: test_ladders(),
            physics: player_physics(),
        },
    );

    assert!(!step.blocked);
    assert!(step.position.z < pos.z);
}

#[test]
fn collider_y_offset_allows_movement_off_ramp_side() {
    let ramp = test_ramp();
    let floor = upper_floor_west_of_ramp();
    let collision_world = collision_world(&[floor], &[ramp]);
    let y = ramp_surface_at(&ramp, 2.0, 7.0);
    let pos = Position { x: 2.0, y, z: 7.0 };
    let motion = 0.0;

    let step = step_character_movement(
        CharacterStep {
            start: pos,
            vertical_velocity: motion,
            target_x: -1.0,
            target_z: pos.z,
            delta: 0.1,
        },
        &CharacterEnvironment {
            collision_world: &collision_world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladders: test_ladders(),
            physics: player_physics(),
        },
    );

    assert!(!step.blocked);
    assert!(step.position.x < pos.x);
}
