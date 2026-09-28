use bevy::math::Vec3;
use bevy::prelude::Bundle;

use common::{
    physics::{CharacterSupport, CharacterVerticalVelocity, HorizontalVelocity, KnockbackVelocity},
    protocol::{CarrierId, FaceYaw, PlayerMoveIntent, PlayerMovementState, Position},
};

#[must_use]
pub fn player_movement_state(
    pos: Position,
    move_intent: PlayerMoveIntent,
    face_yaw: &FaceYaw,
    vertical_velocity: &CharacterVerticalVelocity,
    horizontal_velocity: &HorizontalVelocity,
    knockback: &KnockbackVelocity,
    support: CharacterSupport,
    stance: common::protocol::PlayerStance,
) -> PlayerMovementState {
    PlayerMovementState {
        carrier: CarrierId::WORLD,
        pos,
        move_intent,
        vertical_velocity: vertical_velocity.0,
        face_yaw: face_yaw.0,
        horizontal_velocity: horizontal_velocity.0.to_array(),
        knockback: knockback.0.to_array(),
        support,
        stance,
    }
}

// A movement state's components other than its position.
#[derive(Bundle)]
pub struct PlayerMotionBundle {
    pub move_intent: PlayerMoveIntent,
    pub face_yaw: FaceYaw,
    pub vertical_velocity: CharacterVerticalVelocity,
    pub horizontal_velocity: HorizontalVelocity,
    pub knockback: KnockbackVelocity,
    pub support: CharacterSupport,
    pub stance: common::protocol::PlayerStance,
}

impl From<&PlayerMovementState> for PlayerMotionBundle {
    fn from(movement: &PlayerMovementState) -> Self {
        Self {
            move_intent: movement.move_intent,
            face_yaw: FaceYaw(movement.face_yaw),
            vertical_velocity: CharacterVerticalVelocity(movement.vertical_velocity),
            horizontal_velocity: HorizontalVelocity(Vec3::from_array(movement.horizontal_velocity)),
            knockback: KnockbackVelocity(Vec3::from_array(movement.knockback)),
            support: movement.support,
            stance: movement.stance,
        }
    }
}

#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;
