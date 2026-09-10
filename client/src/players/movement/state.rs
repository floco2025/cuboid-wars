use bevy::math::Vec3;
use bevy::prelude::Bundle;

use common::{
    physics::{AirborneMomentum, CharacterSupport, CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{CarrierId, FaceYaw, PlayerMoveIntent, PlayerMovementState, Position},
};

#[must_use]
pub fn player_movement_state(
    pos: Position,
    move_intent: PlayerMoveIntent,
    face_yaw: &FaceYaw,
    vertical_velocity: &CharacterVerticalVelocity,
    airborne_momentum: &AirborneMomentum,
    knockback: &KnockbackVelocity,
    support: CharacterSupport,
) -> PlayerMovementState {
    PlayerMovementState {
        carrier: CarrierId::WORLD,
        pos,
        move_intent,
        vertical_velocity: vertical_velocity.0,
        face_yaw: face_yaw.0,
        airborne_momentum: airborne_momentum.0.to_array(),
        knockback: knockback.0.to_array(),
        support,
    }
}

// A movement state's components other than its position.
#[derive(Bundle)]
pub struct PlayerMotionBundle {
    pub move_intent: PlayerMoveIntent,
    pub face_yaw: FaceYaw,
    pub vertical_velocity: CharacterVerticalVelocity,
    pub airborne_momentum: AirborneMomentum,
    pub knockback: KnockbackVelocity,
    pub support: CharacterSupport,
}

impl From<&PlayerMovementState> for PlayerMotionBundle {
    fn from(movement: &PlayerMovementState) -> Self {
        Self {
            move_intent: movement.move_intent,
            face_yaw: FaceYaw(movement.face_yaw),
            vertical_velocity: CharacterVerticalVelocity(movement.vertical_velocity),
            airborne_momentum: AirborneMomentum(Vec3::from_array(movement.airborne_momentum)),
            knockback: KnockbackVelocity(Vec3::from_array(movement.knockback)),
            support: movement.support,
        }
    }
}

#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;
