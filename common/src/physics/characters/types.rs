use bevy_ecs::prelude::*;
use bevy_math::Vec3;

use crate::{config::CharacterPhysicsConfig, constants::PHYSICS_EPSILON, protocol::Position};

// Component attached to character entities tracking persistent gravity-axis
// velocity. X/Z velocity is derived from intent each tick. Running on a ramp can
// add vertical displacement for that frame, but it is not stored as velocity.
#[derive(Component, Default)]
pub struct CharacterVerticalVelocity(pub f32);

// Horizontal blast shove, decaying linearly to zero. Movement planning (server
// and client prediction) reads `step` as extra displacement on top of the
// intent-derived target; the decay systems tick it down after movement so
// both sides integrate the same curve. The vertical part of a launch rides
// `CharacterVerticalVelocity` instead.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct KnockbackVelocity(pub Vec3);

impl KnockbackVelocity {
    #[must_use]
    pub fn step(&self, delta: f32) -> Vec3 {
        self.0 * delta
    }

    pub fn decay(&mut self, delta: f32, deceleration: f32) {
        let speed = self.0.length();
        if speed <= PHYSICS_EPSILON {
            self.0 = Vec3::ZERO;
            return;
        }
        // Linear friction-style deceleration: hits zero exactly instead of
        // trailing off into an exponential crawl.
        let remaining = (speed - deceleration * delta).max(0.0);
        self.0 *= remaining / speed;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterMovementResult {
    pub position: Position,
    pub vertical_velocity: f32,
    // True when static-world collision materially blocked requested movement.
    // Side contacts that Rapier resolves by auto-stepping are not treated as blocked.
    pub blocked: bool,
}

// Represents a character's intended movement after static-world collision but
// before character-character collision.
#[derive(Copy, Clone)]
pub struct CharacterMovePlan {
    pub entity: Entity,
    pub start: Position,
    pub target: Position,
    pub target_vertical_velocity: f32,
    pub physics: CharacterPhysicsConfig,
    pub blocked: bool,
}

impl CharacterMovePlan {
    #[must_use]
    pub const fn from_movement_result(
        entity: Entity,
        start: Position,
        step: CharacterMovementResult,
        physics: CharacterPhysicsConfig,
    ) -> Self {
        Self {
            entity,
            start,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            physics,
            blocked: step.blocked,
        }
    }

    #[must_use]
    pub const fn from_target(
        entity: Entity,
        start: Position,
        target: Position,
        target_vertical_velocity: f32,
        physics: CharacterPhysicsConfig,
        blocked: bool,
    ) -> Self {
        Self {
            entity,
            start,
            target,
            target_vertical_velocity,
            physics,
            blocked,
        }
    }

    #[must_use]
    pub const fn stationary(
        entity: Entity,
        position: Position,
        target_vertical_velocity: f32,
        physics: CharacterPhysicsConfig,
    ) -> Self {
        Self::from_target(entity, position, position, target_vertical_velocity, physics, false)
    }

    #[must_use]
    pub const fn with_blocked_xz(mut self) -> Self {
        self.target.x = self.start.x;
        self.target.z = self.start.z;
        self
    }
}
