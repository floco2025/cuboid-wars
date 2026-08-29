use bincode::{Decode, Encode};

use super::BarrierKindId;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ItemType {
    SpeedPowerUp,
    MultiShotPowerUp,
    LowGravityPowerUp,
    // Instant heal on pickup; no durable state on `PlayerInfo` (unlike the
    // other power-ups, which arm a timer). The heal amount comes from
    // `combat.health.player.potion_heal` in the server config.
    HealthPotion,
    Cookie,
    // Key, parameterized by the barrier kind it eventually unlocks. Placed
    // in the map's `items` list; once collected, the kind enters the
    // player's permanent inventory.
    Key(BarrierKindId),
    // Missile ammo. Not collectable while the player is at max — the pack
    // stays in the world, like an already-held key.
    MissilePack,
}

impl ItemType {
    // Config id of the key variant. `from_config_id` deliberately rejects
    // it — a key needs a barrier kind, which a bare config string can't
    // carry — so key-accepting parsers must check this id themselves.
    pub const KEY_CONFIG_ID: &'static str = "key";

    // Items that arm a per-player timer on pickup (the timer
    // power-ups). `HealthPotion` is NOT one of these — its effect is
    // instant; see `PowerUpKind`.
    #[must_use]
    pub const fn is_timer_power_up(self) -> bool {
        matches!(
            self,
            Self::SpeedPowerUp | Self::MultiShotPowerUp | Self::LowGravityPowerUp
        )
    }

    #[must_use]
    pub fn from_config_id(id: &str) -> Option<Self> {
        match id {
            "speed" => Some(Self::SpeedPowerUp),
            "multi_shot" => Some(Self::MultiShotPowerUp),
            "low_gravity" => Some(Self::LowGravityPowerUp),
            "health_potion" => Some(Self::HealthPotion),
            "cookie" => Some(Self::Cookie),
            "missile_pack" => Some(Self::MissilePack),
            _ => None,
        }
    }

    #[must_use]
    pub const fn config_id(self) -> &'static str {
        match self {
            Self::SpeedPowerUp => "speed",
            Self::MultiShotPowerUp => "multi_shot",
            Self::LowGravityPowerUp => "low_gravity",
            Self::HealthPotion => "health_potion",
            Self::Cookie => "cookie",
            Self::Key(_) => Self::KEY_CONFIG_ID,
            Self::MissilePack => "missile_pack",
        }
    }
}

// Timer-based power-up kinds — collected as items, arm a per-kind countdown
// on the player. Indexed by `PowerUpKind::index()` for `[T; PowerUpKind::COUNT]`
// arrays on `PlayerInfo`, `Player`, and `SPlayerStatus`. `HealthPotion` is
// deliberately NOT in this enum: it's an instant-effect item that mutates
// `Health` directly and has no durable flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum PowerUpKind {
    Speed,
    MultiShot,
    LowGravity,
}

impl PowerUpKind {
    pub const COUNT: usize = 3;
    pub const ALL: [PowerUpKind; Self::COUNT] = [Self::Speed, Self::MultiShot, Self::LowGravity];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn from_item_type(ty: ItemType) -> Option<Self> {
        match ty {
            ItemType::SpeedPowerUp => Some(Self::Speed),
            ItemType::MultiShotPowerUp => Some(Self::MultiShot),
            ItemType::LowGravityPowerUp => Some(Self::LowGravity),
            ItemType::HealthPotion | ItemType::Cookie | ItemType::Key(_) | ItemType::MissilePack => None,
        }
    }

    #[must_use]
    pub const fn to_item_type(self) -> ItemType {
        match self {
            Self::Speed => ItemType::SpeedPowerUp,
            Self::MultiShot => ItemType::MultiShotPowerUp,
            Self::LowGravity => ItemType::LowGravityPowerUp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_type_config_ids_round_trip() {
        let non_key = [
            ItemType::SpeedPowerUp,
            ItemType::MultiShotPowerUp,
            ItemType::LowGravityPowerUp,
            ItemType::HealthPotion,
            ItemType::Cookie,
            ItemType::MissilePack,
        ];
        for item_type in non_key {
            assert_eq!(ItemType::from_config_id(item_type.config_id()), Some(item_type));
        }
        assert_eq!(ItemType::Key(BarrierKindId(0)).config_id(), ItemType::KEY_CONFIG_ID);
        assert_eq!(ItemType::from_config_id(ItemType::KEY_CONFIG_ID), None);
    }
}
