use std::slice::from_ref;

use super::*;
use common::config::{CharacterPhysicsConfig, HitboxConfig, MovementColliderConfig};

fn physics(width: f32, depth: f32) -> CharacterPhysicsConfig {
    CharacterPhysicsConfig {
        hitbox: HitboxConfig {
            width,
            height: 1.0,
            depth,
            bottom_offset: 0.0,
        },
        movement_collider: MovementColliderConfig {
            diameter: width.min(depth),
            height: 1.8_f32.max(width.min(depth)),
        },
    }
}

fn pos(x: f32) -> Position {
    Position { x, y: 0.0, z: 0.0 }
}

#[test]
fn stationary_blocker_uses_its_own_collider_size() {
    let small = physics(0.3, 0.3);
    let large = physics(2.0, 2.0);
    let mover = Entity::from_bits(1);
    let blocker = Entity::from_bits(2);
    // Small character stepping toward a blocker that is out of reach of
    // a small collider but inside the volume of a large one.
    let candidate = CharacterMovePlan::from_target(mover, pos(0.0), pos(0.5), 0.0, small, false);

    assert!(character_move_plan_is_blocked(
        &candidate,
        &[],
        &[(blocker, pos(1.3), large)]
    ));
    assert!(!character_move_plan_is_blocked(
        &candidate,
        &[],
        &[(blocker, pos(1.3), small)]
    ));
}

#[test]
fn actor_nearly_touching_a_waiting_climber_can_move_away_but_not_through_it() {
    let physics = physics(1.0, 1.0);
    let start = Position {
        x: 0.0,
        y: 4.400_002,
        z: 0.230_000_1,
    };
    let waiting = CharacterMovePlan::stationary(
        Entity::from_bits(2),
        Position {
            x: 0.000_007_159,
            y: 4.066_666,
            z: -0.77,
        },
        0.0,
        physics,
    );
    for (distance, blocked) in [(0.16, false), (-0.16, true)] {
        let candidate = CharacterMovePlan::from_target(
            Entity::from_bits(1),
            start,
            Position {
                z: start.z + distance,
                ..start
            },
            0.0,
            physics,
            false,
        );
        assert_eq!(
            blocking_character_move_plan(&candidate, from_ref(&waiting)).is_some(),
            blocked
        );
    }
}
