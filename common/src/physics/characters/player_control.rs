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
mod tests {
    use super::*;
    use crate::config::{KnockbackConfig, PlayerMovementConfig};
    use std::collections::HashMap;

    fn map_movement() -> MapMovementConfig {
        MapMovementConfig {
            player: PlayerMovementConfig {
                walk_speed: 4.0,
                run_speed: 7.0,
                speed_power_up: 1.5,
            },
            actors: HashMap::new(),
            missile_speed: 16.0,
            projectile_speed: 90.0,
            gravity: 25.0,
            low_gravity: 5.0,
            ladder_climb_ratio: 0.4,
            knockback: KnockbackConfig {
                max_speed: 15.0,
                up_speed: 7.0,
                deceleration: 35.0,
            },
        }
    }

    #[test]
    fn disabled_player_has_no_control_velocity() {
        let movement = map_movement();
        let intent = PlayerMoveIntent::Running { direction: 0.0 };

        assert_eq!(player_control_velocity(intent, &movement, true, true), Vec3::ZERO);
    }

    #[test]
    fn enabled_player_uses_configured_speed() {
        let movement = map_movement();
        let intent = PlayerMoveIntent::Walking { direction: 0.0 };

        assert_eq!(
            player_control_velocity(intent, &movement, false, false),
            Vec3::Z * movement.player.walk_speed
        );
    }
}
