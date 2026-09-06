use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Timer, TimerMode};
use rapier3d::prelude::ColliderHandle;

use crate::{
    config::ProjectilesConfig,
    constants::PHYSICS_EPSILON,
    physics::CollisionWorld,
    protocol::{BarrierKindId, Position},
};

#[derive(Component)]
pub struct ProjectileMotion {
    pub velocity: Vec3,
    pub lifetime: Timer,
    // Steep shots spawn inside the shooter's own collider (eye position
    // pushed along the aim direction), so self-hits are ignored until the
    // projectile has fully left the shooter's hitbox once. After that the
    // shooter is a valid target — bounce-backs can hit you.
    pub left_shooter: bool,
    // Flight tuning copied from `ProjectilesConfig` at spawn so the pure
    // motion methods need no config threading.
    pub(super) radius: f32,
    pub(super) drag_factor: f32,
    pub(super) bounce_retention: f32,
}

impl ProjectileMotion {
    #[must_use]
    pub fn new(face_yaw: f32, face_pitch: f32, speed: f32, config: &ProjectilesConfig) -> Self {
        Self::from_velocity(
            crate::math::direction_from_yaw_pitch(face_yaw, face_pitch) * speed,
            config,
        )
    }

    #[must_use]
    pub fn from_velocity(velocity: Vec3, config: &ProjectilesConfig) -> Self {
        Self {
            velocity,
            lifetime: Timer::from_seconds(config.lifetime_secs, TimerMode::Once),
            left_shooter: false,
            radius: config.radius,
            drag_factor: config.drag_factor,
            bounce_retention: config.bounce_retention,
        }
    }

    pub fn apply_gravity(&mut self, delta: f32, gravity: f32) {
        if gravity > 0.0 {
            self.velocity.y -= gravity * delta;
        }
    }

    pub fn apply_drag(&mut self, delta: f32) {
        if self.drag_factor > 0.0 {
            let speed = self.velocity.length();
            if speed > PHYSICS_EPSILON {
                let deceleration = self.drag_factor * speed * speed;
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

        let retention = 1.0 - cos_impact * (1.0 - self.bounce_retention);
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
        open_kinds: &[BarrierKindId],
    ) -> Option<f32> {
        let translation = self.velocity * delta;
        collision_world
            .cast_moving_ball_against_barriers(Vec3::from(*projectile_pos), translation, self.radius, open_kinds)
            .map(|hit| hit.t)
    }

    // Fraction of this tick's travel at which the straight path first meets a
    // bounce surface (wall/floor/ramp/powered bridge), if any, without
    // mutating velocity.
    #[must_use]
    pub fn surface_collision_t(
        &self,
        projectile_pos: &Position,
        delta: f32,
        collision_world: &CollisionWorld,
        excluded_colliders: &[ColliderHandle],
    ) -> Option<f32> {
        let translation = self.velocity * delta;
        collision_world
            .cast_moving_ball_excluding(
                Vec3::from(*projectile_pos),
                translation,
                self.radius,
                excluded_colliders,
            )
            .map(|hit| hit.t)
    }

    #[must_use]
    pub fn bounce_at_world_surface(
        &mut self,
        projectile_pos: &Position,
        delta: f32,
        collision_world: &CollisionWorld,
        excluded_colliders: &[ColliderHandle],
    ) -> Option<SurfaceBounce> {
        let translation = self.velocity * delta;
        let collision = collision_world.cast_moving_ball_excluding(
            Vec3::from(*projectile_pos),
            translation,
            self.radius,
            excluded_colliders,
        )?;
        let (position, remaining_delta) =
            self.step_after_collision(projectile_pos, delta, collision.normal, collision.t);
        Some(SurfaceBounce {
            position,
            remaining_delta,
            contact: collision.contact,
            normal: collision.normal,
        })
    }

    // Barriers terminate projectiles (no bounce). Cast against barrier
    // colliders only; if the projectile's straight-line trajectory hits one
    // this frame, return the contact data so the caller can despawn and render
    // a surface-aware impact cue.
    // Kinds in `open_kinds` (pressure-plate-open) are skipped — those
    // barriers are gone visually, so projectiles fly through them.
    #[must_use]
    pub fn terminate_at_barrier(
        &self,
        projectile_pos: &Position,
        delta: f32,
        collision_world: &CollisionWorld,
        open_kinds: &[BarrierKindId],
    ) -> Option<BarrierImpact> {
        let translation = self.velocity * delta;
        let hit = collision_world.cast_moving_ball_against_barriers(
            Vec3::from(*projectile_pos),
            translation,
            self.radius,
            open_kinds,
        )?;
        Some(BarrierImpact {
            point: hit.contact,
            normal: hit.normal,
            kind: hit
                .barrier_kind
                .expect("barrier-only shape cast returned a non-barrier collider"),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SurfaceBounce {
    pub position: Position,
    pub remaining_delta: f32,
    pub contact: Vec3,
    pub normal: Vec3,
}

#[derive(Debug, Clone, Copy)]
pub struct BarrierImpact {
    pub point: Vec3,
    pub normal: Vec3,
    pub kind: BarrierKindId,
}
