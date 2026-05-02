use bevy_ecs::prelude::*;

use crate::{config::CharacterPhysicsConfig, protocol::Position};

// Component attached to character entities tracking persistent gravity-axis
// velocity. X/Z velocity is derived from intent each tick. Running on a ramp can
// add vertical displacement for that frame, but it is not stored as velocity.
#[derive(Component, Default)]
pub struct CharacterVerticalVelocity(pub f32);

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
