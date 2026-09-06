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
    pub speed: f32,
    pub multi_shot: f32,
    pub low_gravity: f32,
    pub portal_gun: f32,
}

impl PowerUpsConfig {
    #[must_use]
    pub const fn duration_secs_for(&self, kind: PowerUpKind) -> f32 {
        let secs = &self.duration_secs;
        match kind {
            PowerUpKind::Speed => secs.speed,
            PowerUpKind::MultiShot => secs.multi_shot,
            PowerUpKind::LowGravity => secs.low_gravity,
            PowerUpKind::PortalGun => secs.portal_gun,
        }
    }

    pub(super) fn validate(&self, path: &str) -> Result<()> {
        let secs = &self.duration_secs;
        for (value, name) in [
            (secs.speed, "speed"),
            (secs.multi_shot, "multi_shot"),
            (secs.low_gravity, "low_gravity"),
            (secs.portal_gun, "portal_gun"),
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
    pub speed: f32,
    pub multi_shot: f32,
    pub low_gravity: f32,
    pub portal_gun: f32,
    pub health_potion: f32,
    pub cookie: f32,
    pub key: f32,
    pub missile_pack: f32,
}

impl PlacedItemsConfig {
    #[must_use]
    pub const fn respawn_secs_for(&self, item_type: ItemType) -> f32 {
        let secs = &self.respawn_secs;
        match item_type {
            ItemType::SpeedPowerUp => secs.speed,
            ItemType::MultiShotPowerUp => secs.multi_shot,
            ItemType::LowGravityPowerUp => secs.low_gravity,
            ItemType::PortalGunPowerUp => secs.portal_gun,
            ItemType::HealthPotion => secs.health_potion,
            ItemType::Cookie => secs.cookie,
            ItemType::Key(_) => secs.key,
            ItemType::MissilePack => secs.missile_pack,
        }
    }

    pub(super) fn validate(&self, path: &str) -> Result<()> {
        let secs = &self.respawn_secs;
        for (value, name) in [
            (secs.speed, "speed"),
            (secs.multi_shot, "multi_shot"),
            (secs.low_gravity, "low_gravity"),
            (secs.portal_gun, "portal_gun"),
            (secs.health_potion, "health_potion"),
            (secs.cookie, "cookie"),
            (secs.key, "key"),
            (secs.missile_pack, "missile_pack"),
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
                speed: 0.0,
                multi_shot: 0.0,
                low_gravity: 0.0,
                portal_gun: 0.0,
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
                speed: 1.0,
                multi_shot: 2.0,
                low_gravity: 4.0,
                portal_gun: 0.0,
                health_potion: 5.0,
                cookie: 6.0,
                key: 7.0,
                missile_pack: 8.0,
            },
        };
        assert_eq!(config.respawn_secs_for(ItemType::SpeedPowerUp), 1.0);
        assert_eq!(config.respawn_secs_for(ItemType::MultiShotPowerUp), 2.0);
        assert_eq!(config.respawn_secs_for(ItemType::LowGravityPowerUp), 4.0);
        assert_eq!(config.respawn_secs_for(ItemType::HealthPotion), 5.0);
        assert_eq!(config.respawn_secs_for(ItemType::Cookie), 6.0);
        assert_eq!(
            config.respawn_secs_for(ItemType::Key(common::protocol::BarrierKindId(0))),
            7.0
        );
        assert_eq!(config.respawn_secs_for(ItemType::MissilePack), 8.0);
    }
}
