use crate::config::fixtures;
use std::f32::consts::PI;

use bevy::math::Vec3;
use common::{
    map::Carriers,
    physics::{CharacterMovementResult, CharacterSupport, CollisionWorld},
    protocol::{ActorMoveIntent, CarrierId, Ladder, MapLayout, Position},
};

use crate::{
    actors::movement::{ActorMovementStep, step_actor_movement},
    test_geometry::{LEVEL_HEIGHT, map_settings},
};

// Edge plane at z = 0 spanning x -0.5..0.5, climbable from the -Z rail side.
fn test_ladder() -> Ladder {
    Ladder {
        x1: -0.5,
        z1: 0.0,
        x2: 0.5,
        z2: 0.0,
        nx: 0.0,
        nz: -1.0,
        level: 0,
        levels: 1,
        y: 0.0,
        height: LEVEL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

fn actor_step(
    can_use_ladders: bool,
    intent: ActorMoveIntent,
    velocity: f32,
    start: Position,
) -> CharacterMovementResult {
    let world = CollisionWorld::from_map_layout(&MapLayout {
        ladders: vec![test_ladder()],
        ..Default::default()
    });
    let physics = fixtures::server_config().gameplay_config().player.physics();
    step_actor_movement(ActorMovementStep {
        start,
        vertical_velocity: velocity,
        intent,
        external_displacement: Vec3::ZERO,
        delta: 0.1,
        can_use_ladders,
        physics,
        open_kinds: &[],
        collision_world: &world,
        map_settings: &map_settings(),
        carriers: &Carriers::default(),
    })
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
