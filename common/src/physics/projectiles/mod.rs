use bevy_ecs::prelude::*;
use bevy_math::{Quat, Vec3};
use bevy_time::{Timer, TimerMode};
use rapier3d::{
    parry::{
        query::{ShapeCastOptions, cast_shapes},
        shape::{Ball, Cuboid},
    },
    prelude::{Pose, Vector},
};

use super::world::CollisionWorld;
use crate::{
    config::CharacterPhysicsConfig,
    constants::{
        PHYSICS_EPSILON, PROJECTILE_BOUNCE_RETENTION, PROJECTILE_DRAG_FACTOR, PROJECTILE_GRAVITY, PROJECTILE_LIFETIME,
        PROJECTILE_RADIUS, PROJECTILE_SPEED,
    },
    protocol::Position,
};

// Direction of a projectile hit (normalized XZ vector).
#[derive(Debug, Clone, Copy)]
pub struct HitDirection {
    pub x: f32,
    pub z: f32,
}

const MAX_SURFACE_BOUNCES: usize = 3;

// Component attached to projectile entities to track velocity, lifetime, and bounce behavior
#[derive(Component)]
pub struct ProjectileMotion {
    pub velocity: Vec3,
    pub lifetime: Timer,
}

impl ProjectileMotion {
    #[must_use]
    pub fn new(face_dir: f32, face_pitch: f32) -> Self {
        let pitch_sin = face_pitch.sin();
        let pitch_cos = face_pitch.cos();
        let velocity = Vec3::new(
            face_dir.sin() * pitch_cos * PROJECTILE_SPEED,
            pitch_sin * PROJECTILE_SPEED,
            face_dir.cos() * pitch_cos * PROJECTILE_SPEED,
        );

        Self {
            velocity,
            lifetime: Timer::from_seconds(PROJECTILE_LIFETIME, TimerMode::Once),
        }
    }

    // Applies gravity to the projectile's velocity.
    pub fn apply_gravity(&mut self, delta: f32) {
        if PROJECTILE_GRAVITY > 0.0 {
            self.velocity.y -= PROJECTILE_GRAVITY * delta;
        }
    }

    // Applies air resistance (drag) to the projectile's velocity.
    // Drag force opposes motion and is proportional to velocity squared.
    pub fn apply_drag(&mut self, delta: f32) {
        if PROJECTILE_DRAG_FACTOR > 0.0 {
            let speed = self.velocity.length();
            if speed > PHYSICS_EPSILON {
                // Deceleration magnitude = drag_factor * v^2
                let deceleration = PROJECTILE_DRAG_FACTOR * speed * speed;
                // Apply deceleration opposite to velocity direction
                let speed_reduction = deceleration * delta;
                // Don't reduce speed below zero
                let new_speed = (speed - speed_reduction).max(0.0);
                self.velocity = self.velocity.normalize() * new_speed;
            }
        }
    }

    // Reflect velocity about the surface normal with angle-dependent energy retention.
    fn reflect_with_retention(&mut self, normal: Vec3) {
        // Calculate impact angle: |dot|/speed = cos(angle from surface normal)
        // cos = 1.0 means head-on (perpendicular), cos = 0.0 means glancing (parallel)
        let speed = self.velocity.length();
        let dot = self.velocity.dot(normal);
        let cos_impact = if speed > PHYSICS_EPSILON {
            (dot.abs() / speed).min(1.0)
        } else {
            0.0
        };

        // Reflect velocity: v' = v - 2(v·n)n
        self.velocity -= 2.0 * dot * normal;

        // Apply energy loss based on impact angle:
        // - Head-on (cos=1): full energy loss (use PROJECTILE_BOUNCE_RETENTION)
        // - Glancing (cos=0): minimal energy loss (retention = 1.0)
        let retention = 1.0 - cos_impact * (1.0 - PROJECTILE_BOUNCE_RETENTION);
        self.velocity *= retention;
    }

    // Advance to collision point, reflect velocity, and return the remaining delta time.
    fn step_after_collision(
        &mut self,
        start_pos: &Position,
        delta: f32,
        collision_normal: Vec3,
        collision_t: f32,
    ) -> (Position, f32) {
        let collision_pos = Vec3::from(*start_pos) + self.velocity * delta * collision_t;
        self.reflect_with_retention(collision_normal);

        let remaining_time = delta * (1.0 - collision_t);
        (collision_pos.into(), remaining_time)
    }

    // Applies world bounce physics: reflects velocity off static map surfaces and returns the new position.
    #[must_use]
    pub fn resolve_world_bounces(
        &mut self,
        projectile_pos: &Position,
        delta: f32,
        collision_world: &CollisionWorld,
    ) -> Option<Position> {
        let mut current_pos = *projectile_pos;
        let mut remaining_delta = delta;
        let mut collided = false;

        for _ in 0..MAX_SURFACE_BOUNCES {
            let translation = self.velocity * remaining_delta;
            let Some(collision) =
                collision_world.cast_moving_ball(Vec3::from(current_pos), translation, PROJECTILE_RADIUS)
            else {
                break;
            };

            let (next_pos, next_delta) =
                self.step_after_collision(&current_pos, remaining_delta, collision.normal, collision.t);
            current_pos = next_pos;
            remaining_delta = next_delta;
            collided = true;

            if remaining_delta <= PHYSICS_EPSILON {
                break;
            }
        }

        if collided {
            let final_pos = Vec3::from(current_pos) + self.velocity * remaining_delta;
            Some(final_pos.into())
        } else {
            None
        }
    }
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
    let projectile_shape = Ball::new(PROJECTILE_RADIUS);
    let character_shape = Cuboid::new(Vector::new(
        character_physics.collider.width / 2.0,
        character_physics.collision_height() / 2.0,
        character_physics.collider.depth / 2.0,
    ));
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

    cast_shapes(
        &projectile_pose,
        projectile_velocity,
        &projectile_shape,
        &character_pose,
        Vector::ZERO,
        &character_shape,
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

    Some(HitDirection { x, z })
}

fn oriented_character_pose(pos: &Position, face_dir: f32, physics: CharacterPhysicsConfig) -> Pose {
    Pose::from_parts(
        Vector::new(pos.x, physics.collider_center_y(pos.y), pos.z),
        Quat::from_rotation_y(face_dir),
    )
}

#[cfg(test)]
mod tests;
