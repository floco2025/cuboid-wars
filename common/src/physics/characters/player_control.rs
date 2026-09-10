use bevy_math::Vec3;

use crate::{config::MapMovementConfig, protocol::PlayerMoveIntent};

#[must_use]
pub fn player_control_velocity(
    move_intent: PlayerMoveIntent,
    movement: &MapMovementConfig,
    has_speed_power_up: bool,
    movement_disabled: bool,
) -> Vec3 {
    if movement_disabled {
        return Vec3::ZERO;
    }

    move_intent.to_horizontal_velocity(
        movement.player.walk_speed,
        movement.player.run_speed,
        has_speed_power_up,
        movement.player.speed_power_up,
    )
}

#[cfg(test)]
#[path = "tests/player_control.rs"]
mod tests;
