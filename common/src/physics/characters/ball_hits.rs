use bevy_math::{Quat, Vec3};
use rapier3d::{
    parry::{
        query::{ShapeCastOptions, cast_shapes, intersection_test},
        shape::Ball,
    },
    prelude::{Pose, Vector},
};

use super::geometry::{character_hitbox_center, character_hitbox_shape};
use crate::{
    config::CharacterPhysicsConfig,
    math::{rapier_pose, to_rapier},
    protocol::Position,
};

#[derive(Debug, Clone, Copy)]
pub struct HitDirection {
    pub x: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct BallCharacterHit {
    pub time_of_impact: f32,
    pub direction: HitDirection,
}

#[must_use]
pub fn ball_character_hit(
    ball_pos: &Position,
    ball_velocity: Vec3,
    ball_radius: f32,
    delta: f32,
    character_pos: &Position,
    character_face_yaw: f32,
    character_physics: CharacterPhysicsConfig,
) -> Option<BallCharacterHit> {
    let ball_shape = Ball::new(ball_radius);
    let character_collider = character_hitbox_shape(character_physics);
    let ball_pose = Pose::from_translation(to_rapier(Vec3::from(*ball_pos)));
    let ball_translation = to_rapier(ball_velocity * delta);
    let character_pose = oriented_character_pose(character_pos, character_face_yaw, character_physics);
    let options = ShapeCastOptions {
        max_time_of_impact: 1.0,
        ..ShapeCastOptions::default()
    };

    let hit = cast_shapes(
        &ball_pose,
        ball_translation,
        &ball_shape,
        &character_pose,
        Vector::ZERO,
        &character_collider,
        options,
    )
    .ok()
    .flatten()?;

    let vel_len = ball_velocity.x.hypot(ball_velocity.z);
    let (x, z) = if vel_len > 0.0 {
        (ball_velocity.x / vel_len, ball_velocity.z / vel_len)
    } else {
        (0.0, 0.0)
    };

    Some(BallCharacterHit {
        time_of_impact: hit.time_of_impact,
        direction: HitDirection { x, z },
    })
}

#[must_use]
pub fn ball_overlaps_character(
    ball_pos: &Position,
    ball_radius: f32,
    character_pos: &Position,
    character_face_yaw: f32,
    character_physics: CharacterPhysicsConfig,
) -> bool {
    let ball_shape = Ball::new(ball_radius);
    let character_collider = character_hitbox_shape(character_physics);
    let ball_pose = Pose::from_translation(to_rapier(Vec3::from(*ball_pos)));
    let character_pose = oriented_character_pose(character_pos, character_face_yaw, character_physics);

    intersection_test(&ball_pose, &ball_shape, &character_pose, &character_collider).is_ok_and(|overlaps| overlaps)
}

fn oriented_character_pose(pos: &Position, face_yaw: f32, physics: CharacterPhysicsConfig) -> Pose {
    rapier_pose(character_hitbox_center(*pos, physics), Quat::from_rotation_y(face_yaw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CharacterPhysicsConfig, HitboxConfig, MovementColliderConfig};
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
}
