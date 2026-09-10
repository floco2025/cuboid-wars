use bevy_ecs::prelude::Bundle;
use bevy_math::Vec3;

use super::{
    momentum::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity},
    types::CharacterSupport,
};
use crate::protocol::{CarrierId, FaceYaw, PlayerMoveIntent, PlayerMovementState, Position};

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
mod tests {
    use super::*;

    #[test]
    fn movement_state_round_trips_through_its_components() {
        let state = PlayerMovementState {
            carrier: CarrierId::WORLD,
            pos: Position { x: 1.0, y: 2.0, z: 3.0 },
            move_intent: PlayerMoveIntent::Running { direction: 0.5 },
            vertical_velocity: -4.0,
            face_yaw: 1.25,
            airborne_momentum: [1.0, 0.0, -2.0],
            knockback: [0.5, 0.0, 0.25],
            support: CharacterSupport::Ladder,
        };
        let bundle = PlayerMotionBundle::from(&state);
        let rebuilt = player_movement_state(
            state.pos,
            bundle.move_intent,
            &bundle.face_yaw,
            &bundle.vertical_velocity,
            &bundle.airborne_momentum,
            &bundle.knockback,
            bundle.support,
        );
        assert_eq!(rebuilt.pos, state.pos);
        assert_eq!(rebuilt.move_intent, state.move_intent);
        assert_eq!(rebuilt.vertical_velocity, state.vertical_velocity);
        assert_eq!(rebuilt.face_yaw, state.face_yaw);
        assert_eq!(rebuilt.airborne_momentum, state.airborne_momentum);
        assert_eq!(rebuilt.knockback, state.knockback);
        assert_eq!(rebuilt.support, state.support);
    }
}
