use bevy::math::{Quat, Vec3};
use rapier3d::{
    parry::{
        query::{ShapeCastOptions, cast_shapes, intersection_test},
        shape::Ball,
    },
    prelude::{Pose, Vector},
};

use common::{
    config::CharacterPhysicsConfig,
    math::{rapier_pose, to_rapier},
    physics::{character_hitbox_center, character_hitbox_shape},
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
#[path = "tests/ball_hits.rs"]
mod tests;
