use anyhow::Result;
use bevy::prelude::Resource;
use serde::Deserialize;

use super::validation::validate_non_negative_finite;
use common::protocol::{ItemType, PowerUpKind};

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct PowerUpsConfig {
    pub duration_secs: PowerUpDurationSecs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PowerUpDurationSecs {
    pub single_shot: f32,
    pub multi_shot: f32,
    pub portal_gun: f32,
    pub speed: f32,
    pub low_gravity: f32,
}

impl PowerUpsConfig {
    #[must_use]
    pub const fn duration_secs_for(&self, kind: PowerUpKind) -> f32 {
        let secs = &self.duration_secs;
        match kind {
            PowerUpKind::SingleShot => secs.single_shot,
            PowerUpKind::MultiShot => secs.multi_shot,
            PowerUpKind::PortalGun => secs.portal_gun,
            PowerUpKind::Speed => secs.speed,
            PowerUpKind::LowGravity => secs.low_gravity,
        }
    }

    pub(super) fn validate(&self, path: &str) -> Result<()> {
        let secs = &self.duration_secs;
        for (value, name) in [
            (secs.single_shot, "single_shot"),
            (secs.multi_shot, "multi_shot"),
            (secs.portal_gun, "portal_gun"),
            (secs.speed, "speed"),
            (secs.low_gravity, "low_gravity"),
        ] {
            validate_non_negative_finite(value, &format!("{path}.duration_secs.{name}"))?;
        }
        Ok(())
    }
}

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct PlacedItemsConfig {
    pub respawn_secs: PlacedItemRespawnSecs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlacedItemRespawnSecs {
    pub single_shot: f32,
    pub multi_shot: f32,
    pub missile_pack: f32,
    pub portal_gun: f32,
    pub health_potion: f32,
    pub speed: f32,
    pub low_gravity: f32,
    pub gold: f32,
    pub key: f32,
}

impl PlacedItemsConfig {
    #[must_use]
    pub const fn respawn_secs_for(&self, item_type: ItemType) -> f32 {
        let secs = &self.respawn_secs;
        match item_type {
            ItemType::SingleShotPowerUp => secs.single_shot,
            ItemType::MultiShotPowerUp => secs.multi_shot,
            ItemType::MissilePack => secs.missile_pack,
            ItemType::PortalGunPowerUp => secs.portal_gun,
            ItemType::HealthPotion => secs.health_potion,
            ItemType::SpeedPowerUp => secs.speed,
            ItemType::LowGravityPowerUp => secs.low_gravity,
            ItemType::Gold => secs.gold,
            ItemType::Key(_) => secs.key,
        }
    }

    pub(super) fn validate(&self, path: &str) -> Result<()> {
        let secs = &self.respawn_secs;
        for (value, name) in [
            (secs.single_shot, "single_shot"),
            (secs.multi_shot, "multi_shot"),
            (secs.missile_pack, "missile_pack"),
            (secs.portal_gun, "portal_gun"),
            (secs.health_potion, "health_potion"),
            (secs.speed, "speed"),
            (secs.low_gravity, "low_gravity"),
            (secs.gold, "gold"),
            (secs.key, "key"),
        ] {
            validate_non_negative_finite(value, &format!("{path}.respawn_secs.{name}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_accept_zero_and_reject_negative_or_non_finite_values() {
        let mut config = PowerUpsConfig {
            duration_secs: PowerUpDurationSecs {
                single_shot: 0.0,
                multi_shot: 0.0,
                portal_gun: 0.0,
                speed: 0.0,
                low_gravity: 0.0,
            },
        };
        assert!(config.validate("maps.test.power_ups").is_ok());
        for invalid in [-1.0, f32::NAN, f32::INFINITY] {
            config.duration_secs.portal_gun = invalid;
            let error = config
                .validate("maps.test.power_ups")
                .expect_err("invalid duration accepted");
            assert!(
                error
                    .to_string()
                    .contains("maps.test.power_ups.duration_secs.portal_gun")
            );
        }
    }

    #[test]
    fn placed_item_respawn_secs_matches_item_type() {
        let config = PlacedItemsConfig {
            respawn_secs: PlacedItemRespawnSecs {
                single_shot: 3.0,
                multi_shot: 2.0,
                missile_pack: 8.0,
                portal_gun: 0.0,
                health_potion: 5.0,
                speed: 1.0,
                low_gravity: 4.0,
                gold: 6.0,
                key: 7.0,
            },
        };
        assert_eq!(config.respawn_secs_for(ItemType::SpeedPowerUp), 1.0);
        assert_eq!(config.respawn_secs_for(ItemType::SingleShotPowerUp), 3.0);
        assert_eq!(config.respawn_secs_for(ItemType::MultiShotPowerUp), 2.0);
        assert_eq!(config.respawn_secs_for(ItemType::LowGravityPowerUp), 4.0);
        assert_eq!(config.respawn_secs_for(ItemType::HealthPotion), 5.0);
        assert_eq!(config.respawn_secs_for(ItemType::Gold), 6.0);
        assert_eq!(
            config.respawn_secs_for(ItemType::Key(common::protocol::BarrierKindId(0))),
            7.0
        );
        assert_eq!(config.respawn_secs_for(ItemType::MissilePack), 8.0);
    }
}
