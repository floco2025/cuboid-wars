use bevy_math::Quat;
use rapier3d::{
    parry::{
        query::{ShapeCastOptions, cast_shapes},
        shape::Ball,
    },
    prelude::{Pose, Vector},
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
pub fn projectile_hits_character(
    proj_pos: &Position,
    proj_motion: &ProjectileMotion,
    delta: f32,
    character_pos: &Position,
    character_face_dir: f32,
    character_physics: CharacterPhysicsConfig,
) -> Option<HitDirection> {
    projectile_character_hit(
        proj_pos,
        proj_motion,
        delta,
        character_pos,
        character_face_dir,
        character_physics,
    )
    .map(|hit| hit.direction)
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
    let projectile_shape = Ball::new(PROJECTILE_RADIUS);
    let character_collider = character_shape(character_physics);
    let projectile_pose = Pose::translation(proj_pos.x, proj_pos.y, proj_pos.z);
    let projectile_velocity = Vector::new(
        proj_motion.velocity.x * delta,
        proj_motion.velocity.y * delta,
        proj_motion.velocity.z * delta,
    );
    let character_pose = oriented_character_pose(character_pos, character_face_dir, character_physics);
    let options = ShapeCastOptions {
        max_time_of_impact: 1.0,
        ..ShapeCastOptions::default()
    };

    let hit = cast_shapes(
        &projectile_pose,
        projectile_velocity,
        &projectile_shape,
        &character_pose,
        Vector::ZERO,
        &character_collider,
        options,
    )
    .ok()
    .flatten()?;

    let vel_len = proj_motion.velocity.x.hypot(proj_motion.velocity.z);
    let (x, z) = if vel_len > 0.0 {
        (proj_motion.velocity.x / vel_len, proj_motion.velocity.z / vel_len)
    } else {
        (0.0, 0.0)
    };

    Some(ProjectileCharacterHit {
        time_of_impact: hit.time_of_impact,
        direction: HitDirection { x, z },
    })
}

fn oriented_character_pose(pos: &Position, face_dir: f32, physics: CharacterPhysicsConfig) -> Pose {
    Pose::from_parts(
        Vector::new(pos.x, physics.collider_center_y(pos.y), pos.z),
        Quat::from_rotation_y(face_dir),
    )
}
