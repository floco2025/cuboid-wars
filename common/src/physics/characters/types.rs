use bevy_ecs::prelude::{Component, Entity};
use bevy_math::Vec3;
use bincode::{Decode, Encode};

use crate::{config::CharacterPhysicsConfig, physics::world::ShapeCastHit, protocol::Position};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterMovementResult {
    pub grounding: GroundingDiagnostics,
    pub position: Position,
    pub vertical_velocity: f32,
    pub support: CharacterSupport,
    // True when static-world collision materially blocked requested movement.
    // Side contacts that Rapier resolves by auto-stepping are not treated as blocked.
    pub blocked: bool,
    // Velocity of the carrier that carried the body this step, zero
    // otherwise. Its vertical part is already in `vertical_velocity` when the
    // body ends airborne; the horizontal part becomes `AirborneMomentum`.
    pub floor_velocity: Vec3,
    // Includes carried portal transit, which does not contribute floor velocity.
    pub lifted: bool,
    // The body ended the step inside a carrier's geometry: a carrier moved
    // into it and the collision could not push it clear. The server kills
    // a crushed body; nothing tunnels through a carrier.
    pub crushed: bool,
}

#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct GroundingDiagnostics {
    pub origin: Vec3,
    pub distance: f32,
    pub hit: Option<ShapeCastHit>,
    pub supported: bool,
}

// Derived independently each step and never read back by the movement motor.
// The owning client reports it with its movement; the server adopts it with
// an accepted position instead of probing again.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum CharacterSupport {
    Airborne,
    Ground,
    Ladder,
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
    pub crushed: bool,
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
            crushed: step.crushed,
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
            crushed: false,
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
