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
    // Steep shots spawn inside the shooter's own collider (eye position
    // pushed along the aim direction), so self-hits are ignored until the
    // projectile has fully left the shooter's hitbox once. After that the
    // shooter is a valid target — bounce-backs can hit you.
    pub left_shooter: bool,
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
            left_shooter: false,
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

    // Fraction of this tick's travel at which the straight path first meets a
    // closed barrier, if any (`open_kinds` are skipped). Lets the caller order
    // barrier termination against a character hit on the same tick.
    #[must_use]
    pub fn barrier_collision_t(
        &self,
        projectile_pos: &Position,
        delta: f32,
        collision_world: &CollisionWorld,
        open_kinds: &[crate::protocol::BarrierKindId],
    ) -> Option<f32> {
        let translation = self.velocity * delta;
        collision_world
            .cast_moving_ball_against_barriers(Vec3::from(*projectile_pos), translation, PROJECTILE_RADIUS, open_kinds)
            .map(|hit| hit.t)
    }

    // Fraction of this tick's travel at which the straight path first meets a
    // bounce surface (wall/floor/ramp), if any. The first-impact time
    // `resolve_world_bounces` would act on, exposed without mutating velocity.
    #[must_use]
    pub fn surface_collision_t(
        &self,
        projectile_pos: &Position,
        delta: f32,
        collision_world: &CollisionWorld,
    ) -> Option<f32> {
        let translation = self.velocity * delta;
        collision_world
            .cast_moving_ball(Vec3::from(*projectile_pos), translation, PROJECTILE_RADIUS)
            .map(|hit| hit.t)
    }

    // Barriers terminate projectiles (no bounce). Cast against barrier
    // colliders only; if the projectile's straight-line trajectory hits one
    // this frame, return the impact position so the caller can despawn.
    // Kinds in `open_kinds` (pressure-plate-open) are skipped — those
    // barriers are gone visually, so projectiles fly through them.
    #[must_use]
    pub fn terminate_at_barrier(
        &self,
        projectile_pos: &Position,
        delta: f32,
        collision_world: &CollisionWorld,
        open_kinds: &[crate::protocol::BarrierKindId],
    ) -> Option<Position> {
        let t = self.barrier_collision_t(projectile_pos, delta, collision_world, open_kinds)?;
        let collision_pos = Vec3::from(*projectile_pos) + self.velocity * delta * t;
        Some(collision_pos.into())
    }

    #[must_use]
    pub fn resolve_world_bounces(
        &mut self,
        projectile_pos: &Position,
        delta: f32,
        collision_world: &CollisionWorld,
    ) -> Option<WorldBounces> {
        let mut current_pos = *projectile_pos;
        let mut remaining_delta = delta;
        let mut first_impact = None;
        let mut budget_exhausted = false;

        for bounce in 0..MAX_SURFACE_BOUNCES {
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
            if first_impact.is_none() {
                first_impact = Some(current_pos);
            }

            if remaining_delta <= PHYSICS_EPSILON {
                break;
            }

            // Ran out of bounce budget with time still left this frame: the
            // tail segment below is unvalidated, so flag it for a final cast.
            // The no-collision and time-exhausted exits leave the tail already
            // verified clear, so they skip the extra cast.
            if bounce == MAX_SURFACE_BOUNCES - 1 {
                budget_exhausted = true;
            }
        }

        let first_impact = first_impact?;

        let translation = self.velocity * remaining_delta;
        let mut final_pos = Vec3::from(current_pos) + translation;
        if budget_exhausted
            && let Some(collision) =
                collision_world.cast_moving_ball(Vec3::from(current_pos), translation, PROJECTILE_RADIUS)
        {
            // Clamp to the surface contact instead of tunneling through it; the
            // next frame resolves the bounce from this valid just-outside pose.
            final_pos = Vec3::from(current_pos) + translation * collision.t;
        }
        Some(WorldBounces {
            position: final_pos.into(),
            first_impact,
        })
    }
}

// Outcome of a tick's surface-bounce resolution.
pub struct WorldBounces {
    // Where the projectile ends the tick, post-bounce travel included.
    pub position: Position,
    // Ball center at the first surface contact — where impact effects
    // belong; `position` can be meters past the surface by tick end.
    pub first_impact: Position,
}
