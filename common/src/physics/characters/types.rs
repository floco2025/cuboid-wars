use bevy_ecs::prelude::{Component, Entity};
use bevy_math::Vec3;
use bincode::{Decode, Encode};

use crate::{
    config::CharacterPhysicsConfig,
    physics::world::ShapeCastHit,
    protocol::{CarrierId, Position},
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CharacterMovementResult {
    pub contact_normals: [Vec3; 5],
    pub grounding: GroundingDiagnostics,
    pub position: Position,
    pub vertical_velocity: f32,
    pub impact_speed: f32,
    pub support: CharacterSupport,
    // True when static-world collision materially blocked requested movement.
    // Side contacts that Rapier resolves by auto-stepping are not treated as blocked.
    pub blocked: bool,
    // The carrier the body rode this step, `CarrierId::WORLD` when none; the
    // frame the owner reports its position in.
    pub carrier: CarrierId,
    // Velocity of the carrier that carried the body this step, zero
    // otherwise. Its vertical part is already in `vertical_velocity` when the
    // body ends airborne; the horizontal part becomes `HorizontalVelocity`.
    pub floor_velocity: Vec3,
    // A carrier moved the body vertically this step.
    pub lifted: bool,
    // The body ended the step inside a carrier's geometry: a carrier moved
    // into it and the collision could not push it clear. The server kills
    // a crushed body; nothing tunnels through a carrier.
    pub crushed: bool,
}

impl CharacterMovementResult {
    // The ride a body still standing on a carrier has on top of its own
    // velocity; a body that left it took the ride into its own velocity.
    pub fn carried(&self) -> Vec3 {
        if self.support == CharacterSupport::Ground {
            self.floor_velocity
        } else {
            Vec3::ZERO
        }
    }
}

#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct GroundingDiagnostics {
    pub origin: Vec3,
    pub distance: f32,
    pub hit: Option<ShapeCastHit>,
    pub supported: bool,
}

// Derived independently each step and never read back by the movement motor.
// The owning client reports it with its movement; the server keeps the report
// instead of probing again.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Encode, Decode)]
pub enum CharacterSupport {
    #[default]
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
    pub impact_speed: f32,
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
            impact_speed: step.impact_speed,
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
            impact_speed: 0.0,
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
}
