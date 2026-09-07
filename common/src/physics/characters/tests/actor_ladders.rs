use std::f32::consts::PI;

use super::*;
use crate::protocol::ActorMoveIntent;

fn actor_step(
    can_use_ladders: bool,
    intent: ActorMoveIntent,
    velocity: f32,
    start: Position,
) -> CharacterMovementResult {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    step_character_movement(
        CharacterStep {
            start,
            vertical_velocity: velocity,
            control_velocity: intent.to_horizontal_velocity(),
            external_displacement: Vec3::ZERO,
            delta: 0.1,
        },
        &CharacterEnvironment {
            collision_world: &world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            physics: player_physics(),
            ladder_climb_ratio: test_ladders(),
            ladder_mode: LadderMode::for_actor(can_use_ladders, intent),
            portals: None,
            carriers: &Carriers::default(),
        },
    )
}

#[test]
fn disabled_actors_ignore_climbing_holding_centering_and_plane_blocking() {
    let start = Position {
        x: 0.3,
        y: 2.0,
        z: -0.5,
    };
    for intent in [
        ActorMoveIntent::Climbing {
            direction: 0.0,
            speed: 5.0,
        },
        ActorMoveIntent::Climbing {
            direction: 0.0,
            speed: 0.0,
        },
    ] {
        let step = actor_step(false, intent, 0.0, start);
        assert_eq!(step.support, CharacterSupport::Airborne);
        assert!(step.position.y < start.y);
        assert_eq!(step.position.x, start.x);
        assert!((step.position.z - (start.z + intent.speed().unwrap_or(0.0) * 0.1)).abs() < 1e-5);
    }
}

#[test]
fn walking_does_not_accidentally_climb_even_for_capable_actors() {
    let start = Position {
        x: 0.0,
        y: 2.0,
        z: -0.5,
    };
    let step = actor_step(
        true,
        ActorMoveIntent::Moving {
            direction: 0.0,
            speed: 5.0,
        },
        0.0,
        start,
    );
    assert_eq!(step.support, CharacterSupport::Airborne);
    assert!(step.position.y < start.y);
    assert!(step.position.z > -0.1);
}

#[test]
fn climbing_actors_stop_and_reverse_without_coasting_upward() {
    let start = Position {
        x: 0.0,
        y: 2.0,
        z: -0.5,
    };
    let hold = actor_step(
        true,
        ActorMoveIntent::Climbing {
            direction: 0.0,
            speed: 0.0,
        },
        3.0,
        start,
    );
    assert_eq!(hold.support, CharacterSupport::Ladder);
    assert_eq!(hold.position.y, start.y);
    assert_eq!(hold.vertical_velocity, 0.0);
    let descend = actor_step(
        true,
        ActorMoveIntent::Climbing {
            direction: PI,
            speed: 5.0,
        },
        3.0,
        start,
    );
    assert_eq!(descend.support, CharacterSupport::Ladder);
    assert!(descend.position.y < start.y);
}

#[test]
fn ladder_exit_keeps_height_and_can_cross_the_plane_at_an_intermediate_landing() {
    let start = Position {
        x: 0.0,
        y: 2.0,
        z: -0.5,
    };
    let step = actor_step(
        true,
        ActorMoveIntent::ExitingLadder {
            direction: 0.0,
            speed: 5.0,
        },
        3.0,
        start,
    );
    assert_eq!(step.position.y, start.y);
    assert!(step.position.z > -0.1);
}
