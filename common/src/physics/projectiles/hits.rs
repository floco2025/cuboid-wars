use crate::{
    config::CharacterPhysicsConfig,
    constants::PROJECTILE_RADIUS,
    physics::characters::{BallCharacterHit, ball_character_hit, ball_overlaps_character},
    protocol::Position,
};

use super::ProjectileMotion;

#[must_use]
pub fn projectile_character_hit(
    proj_pos: &Position,
    proj_motion: &ProjectileMotion,
    delta: f32,
    character_pos: &Position,
    character_face_dir: f32,
    character_physics: CharacterPhysicsConfig,
) -> Option<BallCharacterHit> {
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
