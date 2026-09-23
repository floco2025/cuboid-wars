use bincode::{Decode, Encode};

use super::FieldId;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub enum ItemType {
    SingleShotPowerUp,
    MultiShotPowerUp,
    // Missile ammo. Not collectable while the player is at max — the pack
    // stays in the world, like an already-held key.
    MissilePack,
    PortalGunPowerUp,
    // Instant heal on pickup; no durable state on `PlayerInfo` (unlike the
    // other power-ups, which persist). The heal amount comes from
    // `combat.health.player.potion_heal` in the server config.
    HealthPotion,
    SpeedPowerUp,
    LowGravityPowerUp,
    Gold,
    // Key, parameterized by the field it eventually unlocks. Placed
    // in the map's `items` list; once collected, the kind enters the
    // player's permanent inventory.
    Key(FieldId),
    // Instant removal of collected equipment; no persistent power-up flag.
    EquipmentEraser,
}

impl ItemType {
    // Config id of the key variant. `from_config_id` deliberately rejects
    // it — a key needs a field, which a bare config string can't
    // carry — so key-accepting parsers must check this id themselves.
    pub const KEY_CONFIG_ID: &'static str = "key";

    // Items that grant a persistent player effect on pickup. Instant items
    // such as healing and equipment erasure have no `PowerUpKind` flag.
    #[must_use]
    pub const fn is_power_up(self) -> bool {
        matches!(
            self,
            Self::SingleShotPowerUp
                | Self::MultiShotPowerUp
                | Self::PortalGunPowerUp
                | Self::SpeedPowerUp
                | Self::LowGravityPowerUp
        )
    }

    #[must_use]
    pub fn from_config_id(id: &str) -> Option<Self> {
        match id {
            "single_shot" => Some(Self::SingleShotPowerUp),
            "multi_shot" => Some(Self::MultiShotPowerUp),
            "missile_pack" => Some(Self::MissilePack),
            "portal_gun" => Some(Self::PortalGunPowerUp),
            "health_potion" => Some(Self::HealthPotion),
            "equipment_eraser" => Some(Self::EquipmentEraser),
            "speed" => Some(Self::SpeedPowerUp),
            "low_gravity" => Some(Self::LowGravityPowerUp),
            "gold" => Some(Self::Gold),
            _ => None,
        }
    }

    #[must_use]
    pub const fn config_id(self) -> &'static str {
        match self {
            Self::SingleShotPowerUp => "single_shot",
            Self::MultiShotPowerUp => "multi_shot",
            Self::MissilePack => "missile_pack",
            Self::PortalGunPowerUp => "portal_gun",
            Self::HealthPotion => "health_potion",
            Self::EquipmentEraser => "equipment_eraser",
            Self::SpeedPowerUp => "speed",
            Self::LowGravityPowerUp => "low_gravity",
            Self::Gold => "gold",
            Self::Key(_) => Self::KEY_CONFIG_ID,
        }
    }
}

// Power-up kinds indexed by `PowerUpKind::index()` for `[T; PowerUpKind::COUNT]`
// arrays on `PlayerInfo`, `Player`, and `SPlayerStatus`. Healing and
// equipment erasure are instant effects and have no durable flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum PowerUpKind {
    SingleShot,
    MultiShot,
    PortalGun,
    Speed,
    LowGravity,
}

impl PowerUpKind {
    pub const COUNT: usize = 5;
    pub const ALL: [PowerUpKind; Self::COUNT] = [
        Self::SingleShot,
        Self::MultiShot,
        Self::PortalGun,
        Self::Speed,
        Self::LowGravity,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn from_item_type(ty: ItemType) -> Option<Self> {
        match ty {
            ItemType::SingleShotPowerUp => Some(Self::SingleShot),
            ItemType::MultiShotPowerUp => Some(Self::MultiShot),
            ItemType::PortalGunPowerUp => Some(Self::PortalGun),
            ItemType::SpeedPowerUp => Some(Self::Speed),
            ItemType::LowGravityPowerUp => Some(Self::LowGravity),
            ItemType::HealthPotion
            | ItemType::EquipmentEraser
            | ItemType::Gold
            | ItemType::Key(_)
            | ItemType::MissilePack => None,
        }
    }

    #[must_use]
    pub const fn to_item_type(self) -> ItemType {
        match self {
            Self::SingleShot => ItemType::SingleShotPowerUp,
            Self::MultiShot => ItemType::MultiShotPowerUp,
            Self::PortalGun => ItemType::PortalGunPowerUp,
            Self::Speed => ItemType::SpeedPowerUp,
            Self::LowGravity => ItemType::LowGravityPowerUp,
        }
    }
}

#[cfg(test)]
#[path = "tests/items.rs"]
mod tests;
