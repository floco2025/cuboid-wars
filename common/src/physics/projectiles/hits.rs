use bevy_math::Vec3;
use rapier3d::{
    parry::{
        query::{ShapeCastOptions, cast_shapes, intersection_test},
        shape::Ball,
    },
    prelude::{Pose, Vector, glamx::Quat},
};

use crate::{
    config::CharacterPhysicsConfig, constants::PROJECTILE_RADIUS, physics::characters::character_shape,
    protocol::Position,
};

use super::ProjectileMotion;

#[derive(Debug, Clone, Copy)]
pub struct HitDirection {
    pub x: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectileCharacterHit {
    pub time_of_impact: f32,
    pub direction: HitDirection,
}

#[must_use]
pub fn projectile_character_hit(
    proj_pos: &Position,
    proj_motion: &ProjectileMotion,
    delta: f32,
    character_pos: &Position,
    character_face_dir: f32,
    character_physics: CharacterPhysicsConfig,
) -> Option<ProjectileCharacterHit> {
    ball_character_hit(
        proj_pos,
        proj_motion.velocity,
        PROJECTILE_RADIUS,
        delta,
        character_pos,
        character_face_dir,
        character_physics,
    )
}

#[must_use]
pub fn ball_character_hit(
    ball_pos: &Position,
    ball_velocity: Vec3,
    ball_radius: f32,
    delta: f32,
    character_pos: &Position,
    character_face_dir: f32,
    character_physics: CharacterPhysicsConfig,
) -> Option<ProjectileCharacterHit> {
    let ball_shape = Ball::new(ball_radius);
    let character_collider = character_shape(character_physics);
    let ball_pose = Pose::translation(ball_pos.x, ball_pos.y, ball_pos.z);
    let ball_translation = Vector::new(
        ball_velocity.x * delta,
        ball_velocity.y * delta,
        ball_velocity.z * delta,
    );
    let character_pose = oriented_character_pose(character_pos, character_face_dir, character_physics);
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

    Some(ProjectileCharacterHit {
        time_of_impact: hit.time_of_impact,
        direction: HitDirection { x, z },
    })
}

// True while the projectile's ball overlaps the character's oriented
// collider. Used to gate self-hits: a projectile may only hit its shooter
// after this has gone false once.
#[must_use]
pub fn projectile_overlaps_character(
    proj_pos: &Position,
    character_pos: &Position,
    character_face_dir: f32,
    character_physics: CharacterPhysicsConfig,
) -> bool {
    ball_overlaps_character(
        proj_pos,
        PROJECTILE_RADIUS,
        character_pos,
        character_face_dir,
        character_physics,
    )
}

#[must_use]
pub fn ball_overlaps_character(
    ball_pos: &Position,
    ball_radius: f32,
    character_pos: &Position,
    character_face_dir: f32,
    character_physics: CharacterPhysicsConfig,
) -> bool {
    let ball_shape = Ball::new(ball_radius);
    let character_collider = character_shape(character_physics);
    let ball_pose = Pose::translation(ball_pos.x, ball_pos.y, ball_pos.z);
    let character_pose = oriented_character_pose(character_pos, character_face_dir, character_physics);

    intersection_test(&ball_pose, &ball_shape, &character_pose, &character_collider).is_ok_and(|overlaps| overlaps)
}

fn oriented_character_pose(pos: &Position, face_dir: f32, physics: CharacterPhysicsConfig) -> Pose {
    Pose::from_parts(
        Vector::new(pos.x, physics.collider_center_y(pos.y), pos.z),
        Quat::from_rotation_y(face_dir),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CharacterColliderAnchor, CharacterColliderConfig, CharacterPhysicsConfig, CharacterSupportProbeConfig,
    };

    fn physics() -> CharacterPhysicsConfig {
        CharacterPhysicsConfig {
            collider: CharacterColliderConfig {
                width: 1.0,
                height: 1.3,
                depth: 0.6,
                y_offset: 0.0,
                y_offset_anchor: CharacterColliderAnchor::Bottom,
            },
            support_probe: CharacterSupportProbeConfig { width: 0.2, depth: 0.2 },
        }
    }

    #[test]
    fn overlap_tracks_inside_and_outside_positions() {
        let character_pos = Position::default();
        let inside = Position { x: 0.0, y: 0.6, z: 0.0 };
        let outside = Position { x: 0.0, y: 0.6, z: 2.0 };

        assert!(projectile_overlaps_character(&inside, &character_pos, 0.0, physics()));
        assert!(!projectile_overlaps_character(&outside, &character_pos, 0.0, physics()));
    }

    #[test]
    fn overlap_respects_character_orientation() {
        let character_pos = Position::default();
        // Just past the narrow depth half-extent (0.3 + radius) but inside
        // the wide width half-extent (0.5 + radius) — overlap depends on
        // which axis faces the projectile.
        let probe = Position {
            x: 0.0,
            y: 0.6,
            z: 0.45,
        };

        assert!(!projectile_overlaps_character(&probe, &character_pos, 0.0, physics()));
        assert!(projectile_overlaps_character(
            &probe,
            &character_pos,
            std::f32::consts::FRAC_PI_2,
            physics()
        ));
    }
}
