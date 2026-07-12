use bincode::{Decode, Encode};

use super::ItemType;

// Timer-based power-up kinds — collected as items, arm a per-kind countdown
// on the player. Indexed by `PowerUpKind::index()` for `[T; PowerUpKind::COUNT]`
// arrays on `PlayerInfo`, `Player`, and `SPlayerStatus`. `HealthPotion` is
// deliberately NOT in this enum: it's an instant-effect item that mutates
// `Health` directly and has no durable flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum PowerUpKind {
    Speed,
    MultiShot,
    Phasing,
    LowGravity,
}

impl PowerUpKind {
    pub const COUNT: usize = 4;
    pub const ALL: [PowerUpKind; Self::COUNT] = [Self::Speed, Self::MultiShot, Self::Phasing, Self::LowGravity];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn from_item_type(ty: ItemType) -> Option<Self> {
        match ty {
            ItemType::SpeedPowerUp => Some(Self::Speed),
            ItemType::MultiShotPowerUp => Some(Self::MultiShot),
            ItemType::PhasingPowerUp => Some(Self::Phasing),
            ItemType::LowGravityPowerUp => Some(Self::LowGravity),
            ItemType::HealthPotion | ItemType::Cookie | ItemType::Key(_) => None,
        }
    }

    #[must_use]
    pub const fn to_item_type(self) -> ItemType {
        match self {
            Self::Speed => ItemType::SpeedPowerUp,
            Self::MultiShot => ItemType::MultiShotPowerUp,
            Self::Phasing => ItemType::PhasingPowerUp,
            Self::LowGravity => ItemType::LowGravityPowerUp,
        }
    }
}
