use super::*;
use common::config::{CharacterPhysicsConfig, HitboxConfig, MovementColliderConfig};
use std::f32::consts::FRAC_PI_2;

const BALL_RADIUS: f32 = 0.1;

fn physics() -> CharacterPhysicsConfig {
    CharacterPhysicsConfig {
        hitbox: HitboxConfig {
            width: 1.0,
            height: 1.3,
            depth: 0.6,
            bottom_offset: 0.0,
        },
        movement_collider: MovementColliderConfig {
            diameter: 0.6,
            height: 1.8,
        },
    }
}

#[test]
fn overlap_tracks_inside_and_outside_positions() {
    let character_pos = Position::default();
    let inside = Position { x: 0.0, y: 0.6, z: 0.0 };
    let outside = Position { x: 0.0, y: 0.6, z: 2.0 };

    assert!(ball_overlaps_character(
        &inside,
        BALL_RADIUS,
        &character_pos,
        0.0,
        physics()
    ));
    assert!(!ball_overlaps_character(
        &outside,
        BALL_RADIUS,
        &character_pos,
        0.0,
        physics()
    ));
}

#[test]
fn overlap_respects_character_orientation() {
    let character_pos = Position::default();
    // Just past the narrow depth half-extent (0.3 + radius) but inside
    // the wide width half-extent (0.5 + radius) — overlap depends on
    // which axis faces the ball.
    let probe = Position {
        x: 0.0,
        y: 0.6,
        z: 0.45,
    };

    assert!(!ball_overlaps_character(
        &probe,
        BALL_RADIUS,
        &character_pos,
        0.0,
        physics()
    ));
    assert!(ball_overlaps_character(
        &probe,
        BALL_RADIUS,
        &character_pos,
        FRAC_PI_2,
        physics()
    ));
}
