use anyhow::{Result, ensure};
use bevy::prelude::Resource;
use serde::Deserialize;

use super::validation::{deserialize_required_option, validate_non_negative_finite, validate_positive_finite};
use common::protocol::{ItemType, PowerUpKind};

#[derive(Resource, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerUpsConfig {
    pub single_shot: PowerUpMode,
    pub multi_shot: PowerUpMode,
    pub portal_gun: PowerUpMode,
    pub speed: PowerUpMode,
    pub low_gravity: PowerUpMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum PowerUpMode {
    // A struct variant makes Serde reject pickup-only fields here.
    Always {},
    Pickup {
        #[serde(deserialize_with = "deserialize_required_option")]
        duration_secs: Option<f32>,
    },
}

impl PowerUpsConfig {
    #[must_use]
    pub const fn mode(&self, kind: PowerUpKind) -> PowerUpMode {
        match kind {
            PowerUpKind::SingleShot => self.single_shot,
            PowerUpKind::MultiShot => self.multi_shot,
            PowerUpKind::PortalGun => self.portal_gun,
            PowerUpKind::Speed => self.speed,
            PowerUpKind::LowGravity => self.low_gravity,
        }
    }

    pub fn always_active(&self) -> [bool; PowerUpKind::COUNT] {
        PowerUpKind::ALL.map(|kind| self.mode(kind) == PowerUpMode::Always {})
    }

    pub(crate) fn validate_pickup(&self, item: ItemType, path: &str) -> Result<()> {
        ensure!(
            PowerUpKind::from_item_type(item).is_none_or(|kind| self.mode(kind) != PowerUpMode::Always {}),
            "{path}: {} is always active and cannot be a pickup",
            item.config_id()
        );
        Ok(())
    }

    pub(super) fn validate(&self, path: &str) -> Result<()> {
        for kind in PowerUpKind::ALL {
            if let PowerUpMode::Pickup {
                duration_secs: Some(seconds),
            } = self.mode(kind)
            {
                validate_positive_finite(
                    seconds,
                    &format!("{path}.{}.duration_secs", kind.to_item_type().config_id()),
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Resource, Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedItemsConfig {
    pub respawn_secs: PlacedItemRespawnSecs,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedItemRespawnSecs {
    pub single_shot: Option<f32>,
    pub multi_shot: Option<f32>,
    pub missile_pack: Option<f32>,
    pub portal_gun: Option<f32>,
    pub health_potion: Option<f32>,
    pub equipment_eraser: Option<f32>,
    pub speed: Option<f32>,
    pub low_gravity: Option<f32>,
    pub gold: Option<f32>,
    pub key: Option<f32>,
}

impl PlacedItemsConfig {
    #[must_use]
    pub const fn respawn_secs_for(&self, item_type: ItemType) -> Option<f32> {
        let secs = &self.respawn_secs;
        match item_type {
            ItemType::SingleShotPowerUp => secs.single_shot,
            ItemType::MultiShotPowerUp => secs.multi_shot,
            ItemType::MissilePack => secs.missile_pack,
            ItemType::PortalGunPowerUp => secs.portal_gun,
            ItemType::HealthPotion => secs.health_potion,
            ItemType::EquipmentEraser => secs.equipment_eraser,
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
            (secs.equipment_eraser, "equipment_eraser"),
            (secs.speed, "speed"),
            (secs.low_gravity, "low_gravity"),
            (secs.gold, "gold"),
            (secs.key, "key"),
        ] {
            if let Some(value) = value {
                validate_non_negative_finite(value, &format!("{path}.respawn_secs.{name}"))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/items.rs"]
mod tests;
