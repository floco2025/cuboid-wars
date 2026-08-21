use bevy_math::Vec3;

use crate::{config::GameplayConfig, protocol::PlayerMoveIntent};

#[must_use]
pub fn player_control_velocity(
    move_intent: PlayerMoveIntent,
    gameplay_config: &GameplayConfig,
    has_speed_power_up: bool,
    movement_disabled: bool,
) -> Vec3 {
    if movement_disabled {
        return Vec3::ZERO;
    }

    move_intent.to_horizontal_velocity(
        gameplay_config.player.walk_speed,
        gameplay_config.player.run_speed,
        has_speed_power_up,
        gameplay_config.power_up_effects.speed_multiplier,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_player_has_no_control_velocity() {
        let gameplay = GameplayConfig::load_default().expect("default gameplay config should load");
        let intent = PlayerMoveIntent::Running { direction: 0.0 };

        assert_eq!(player_control_velocity(intent, &gameplay, true, true), Vec3::ZERO);
    }

    #[test]
    fn enabled_player_uses_configured_speed() {
        let gameplay = GameplayConfig::load_default().expect("default gameplay config should load");
        let intent = PlayerMoveIntent::Walking { direction: 0.0 };

        assert_eq!(
            player_control_velocity(intent, &gameplay, false, false),
            Vec3::Z * gameplay.player.walk_speed
        );
    }
}
