use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Timer, TimerMode};

use crate::{
    constants::{
        PHYSICS_EPSILON, PROJECTILE_BOUNCE_RETENTION, PROJECTILE_DRAG_FACTOR, PROJECTILE_GRAVITY, PROJECTILE_LIFETIME,
        PROJECTILE_RADIUS, PROJECTILE_SPEED,
    },
    physics::CollisionWorld,
    protocol::Position,
};

const MAX_SURFACE_BOUNCES: usize = 3;

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

    pub fn apply_gravity(&mut self, delta: f32) {
        if PROJECTILE_GRAVITY > 0.0 {
            self.velocity.y -= PROJECTILE_GRAVITY * delta;
        }
    }

    pub fn apply_drag(&mut self, delta: f32) {
        if PROJECTILE_DRAG_FACTOR > 0.0 {
            let speed = self.velocity.length();
            if speed > PHYSICS_EPSILON {
                let deceleration = PROJECTILE_DRAG_FACTOR * speed * speed;
                let speed_reduction = deceleration * delta;
                let new_speed = (speed - speed_reduction).max(0.0);
                self.velocity = self.velocity.normalize() * new_speed;
            }
        }
    }

    fn reflect_with_retention(&mut self, normal: Vec3) {
        let speed = self.velocity.length();
        let dot = self.velocity.dot(normal);
        let cos_impact = if speed > PHYSICS_EPSILON {
            (dot.abs() / speed).min(1.0)
        } else {
            0.0
        };

        self.velocity -= 2.0 * dot * normal;

        let retention = 1.0 - cos_impact * (1.0 - PROJECTILE_BOUNCE_RETENTION);
        self.velocity *= retention;
    }

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
