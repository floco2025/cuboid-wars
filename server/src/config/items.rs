use anyhow::Result;
use serde::Deserialize;

use super::validation::validate_non_negative_finite;
use common::protocol::{ItemType, PowerUpKind};

#[derive(Debug, Clone, Deserialize)]
pub struct ItemsConfig {
    pub power_ups: PowerUpsConfig,
    pub placed: PlacedItemsConfig,
}

impl ItemsConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        self.power_ups.validate(&format!("{path}.power_ups"))?;
        self.placed.validate(&format!("{path}.placed"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PowerUpsConfig {
    pub duration_secs: PowerUpDurationSecs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PowerUpDurationSecs {
    pub speed: f32,
    pub multi_shot: f32,
    pub low_gravity: f32,
}

impl PowerUpsConfig {
    #[must_use]
    pub const fn duration_secs_for(&self, kind: PowerUpKind) -> f32 {
        let secs = &self.duration_secs;
        match kind {
            PowerUpKind::Speed => secs.speed,
            PowerUpKind::MultiShot => secs.multi_shot,
            PowerUpKind::LowGravity => secs.low_gravity,
        }
    }

    fn validate(&self, path: &str) -> Result<()> {
        let secs = &self.duration_secs;
        for (value, name) in [
            (secs.speed, "speed"),
            (secs.multi_shot, "multi_shot"),
            (secs.low_gravity, "low_gravity"),
        ] {
            validate_non_negative_finite(value, &format!("{path}.duration_secs.{name}"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlacedItemsConfig {
    pub respawn_secs: PlacedItemRespawnSecs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlacedItemRespawnSecs {
    pub speed: f32,
    pub multi_shot: f32,
    pub low_gravity: f32,
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
            ItemType::HealthPotion => secs.health_potion,
            ItemType::Cookie => secs.cookie,
            ItemType::Key(_) => secs.key,
            ItemType::MissilePack => secs.missile_pack,
        }
    }

    fn validate(&self, path: &str) -> Result<()> {
        let secs = &self.respawn_secs;
        for (value, name) in [
            (secs.speed, "speed"),
            (secs.multi_shot, "multi_shot"),
            (secs.low_gravity, "low_gravity"),
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
    fn placed_item_respawn_secs_matches_item_type() {
        let config = PlacedItemsConfig {
            respawn_secs: PlacedItemRespawnSecs {
                speed: 1.0,
                multi_shot: 2.0,
                low_gravity: 4.0,
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
