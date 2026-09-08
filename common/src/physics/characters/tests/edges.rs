use super::*;
use crate::constants::{CHARACTER_CONTACT_OFFSET, TICK_SECS};

fn step_at_edge(world: &CollisionWorld, pos: Position, velocity: f32, control: Vec3) -> CharacterMovementResult {
    step_character_movement(
        CharacterStep {
            start: pos,
            vertical_velocity: velocity,
            control_velocity: control,
            external_displacement: Vec3::ZERO,
            delta: TICK_SECS,
        },
        &CharacterEnvironment {
            ladder_mode: LadderMode::Disabled,
            collision_world: world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
            portals: None,
            carriers: &Carriers::default(),
        },
    )
}

#[test]
fn stable_capsule_overhang_remains_supported_without_forced_slide() {
    let floor = lower_floor();
    let world = collision_world(&[floor], &[]);
    let mut pos = Position {
        x: floor.x2 + player_physics().movement_collider.radius * 0.4,
        y: floor.y,
        z: 0.0,
    };
    let start_x = pos.x;
    let mut velocity = 0.0;
    for _ in 0..60 {
        let step = step_at_edge(&world, pos, velocity, Vec3::ZERO);
        pos = step.position;
        velocity = step.vertical_velocity;
    }
    assert!((pos.x - start_x).abs() < 0.01, "stable contact drifted: {pos:?}");
    assert!(position_has_floor_support(&world, &pos, player_physics()));
    assert!(player_jump_velocity(velocity, &world, player_physics(), 12.0, &pos).is_some());
    assert_eq!(velocity, 0.0);
}

#[test]
fn steep_capsule_edge_contact_slides_off_and_falls() {
    let floor = lower_floor();
    let world = collision_world(&[floor], &[]);
    let mut pos = Position {
        x: floor.x2 + player_physics().movement_collider.radius * 0.95,
        y: floor.y,
        z: 0.0,
    };
    let mut velocity = 0.0;
    for _ in 0..60 {
        let step = step_at_edge(&world, pos, velocity, Vec3::ZERO);
        pos = step.position;
        velocity = step.vertical_velocity;
    }
    assert!(
        pos.x > floor.x2 + player_physics().movement_collider.radius,
        "still on edge: {pos:?}"
    );
    assert!(pos.y < floor.y - 1.0);
    assert!(velocity < 0.0);
    assert!(player_jump_velocity(velocity, &world, player_physics(), 12.0, &pos).is_none());
}

#[test]
fn supported_character_can_walk_back_from_an_overhang() {
    let floor = lower_floor();
    let world = collision_world(&[floor], &[]);
    let mut pos = Position {
        x: floor.x2 + 0.1,
        y: floor.y,
        z: 0.0,
    };
    let mut velocity = 0.0;
    for _ in 0..15 {
        let step = step_at_edge(&world, pos, velocity, -Vec3::X * 3.0);
        pos = step.position;
        velocity = step.vertical_velocity;
    }
    assert!(pos.x < floor.x2 - 1.0);
    assert!((pos.y - floor.y).abs() <= CHARACTER_CONTACT_OFFSET * 2.0);
    assert_eq!(velocity, 0.0);
}

#[test]
fn ground_snap_does_not_undo_a_step_onto_a_raised_edge() {
    let ledge = Floor {
        x2: 1.5,
        y: 0.15,
        ..lower_floor()
    };
    let landing = Floor {
        x1: 1.52,
        ..lower_floor()
    };
    let world = collision_world(&[ledge, landing], &[]);
    let mut pos = Position {
        x: 1.85,
        y: 0.0,
        z: 0.0,
    };
    let mut velocity = 0.0;
    for _ in 0..10 {
        let step = step_at_edge(&world, pos, velocity, -Vec3::X * 3.0);
        pos = step.position;
        velocity = step.vertical_velocity;
    }
    assert!(pos.x < 1.0, "did not step forward: {pos:?}");
    assert!(
        (pos.y - ledge.y).abs() <= CHARACTER_CONTACT_OFFSET * 2.0,
        "not on ledge: {pos:?}"
    );
    assert_eq!(velocity, 0.0);
}

#[test]
fn unsupported_fall_is_not_snapped_to_a_nearby_floor() {
    let world = collision_world(&[lower_floor()], &[]);
    let start = Position {
        x: 0.0,
        y: 0.15,
        z: 0.0,
    };
    let step = step_at_edge(&world, start, 0.0, Vec3::ZERO);
    assert_eq!(step.support, CharacterSupport::Airborne);
    assert!(step.position.y > 0.1);
    assert!(step.vertical_velocity < 0.0);
}
