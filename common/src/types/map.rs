use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};

use super::{BarrierKindId, Position};
use crate::face_materials::FaceMaterials;

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Wall {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub width: f32,
    pub level: u8,
}

impl Wall {
    #[must_use]
    pub const fn bounds_xz(&self) -> (f32, f32, f32, f32) {
        (
            self.x1.min(self.x2),
            self.x1.max(self.x2),
            self.z1.min(self.z2),
            self.z1.max(self.z2),
        )
    }
}

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Floor {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub y: f32,
    pub thickness: f32,
    pub level: u8,
}

impl Floor {
    #[must_use]
    pub const fn bounds_xz(&self) -> (f32, f32, f32, f32) {
        (
            self.x1.min(self.x2),
            self.x1.max(self.x2),
            self.z1.min(self.z2),
            self.z1.max(self.z2),
        )
    }
}

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Ramp {
    pub x1: f32,
    pub y1: f32,
    pub z1: f32,
    pub x2: f32,
    pub y2: f32,
    pub z2: f32,
}

impl Ramp {
    #[must_use]
    pub const fn bounds_xz(&self) -> (f32, f32, f32, f32) {
        (
            self.x1.min(self.x2),
            self.x1.max(self.x2),
            self.z1.min(self.z2),
            self.z1.max(self.z2),
        )
    }

    #[must_use]
    pub const fn bounds_y(&self) -> (f32, f32) {
        (self.y1.min(self.y2), self.y1.max(self.y2))
    }
}

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct WallLight {
    pub pos: Position,
    pub yaw: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ItemType {
    SpeedPowerUp,
    MultiShotPowerUp,
    PhasingPowerUp,
    LowGravityPowerUp,
    // Instant heal on pickup; no durable state on `PlayerInfo` (unlike the
    // other power-ups, which arm a timer). The heal amount comes from
    // `PowerUpsConfig.health_potion_heal_fraction`.
    HealthPotion,
    Cookie,
    // Key, parameterized by the barrier kind it eventually unlocks. Placed
    // in the map's `items` list; once collected, the kind enters the
    // player's permanent inventory.
    Key(BarrierKindId),
}

impl ItemType {
    // Config id of the key variant. `from_config_id` deliberately rejects
    // it — a key needs a barrier kind, which a bare config string can't
    // carry — so key-accepting parsers must check this id themselves.
    pub const KEY_CONFIG_ID: &'static str = "key";

    // Items that arm a per-player timer on pickup (the classic four
    // power-ups). `HealthPotion` is NOT one of these — its effect is
    // instant; see `PowerUpKind`.
    #[must_use]
    pub const fn is_timer_power_up(self) -> bool {
        matches!(
            self,
            Self::SpeedPowerUp | Self::MultiShotPowerUp | Self::PhasingPowerUp | Self::LowGravityPowerUp
        )
    }

    #[must_use]
    pub fn from_config_id(id: &str) -> Option<Self> {
        match id {
            "speed" => Some(Self::SpeedPowerUp),
            "multi_shot" => Some(Self::MultiShotPowerUp),
            "phasing" => Some(Self::PhasingPowerUp),
            "low_gravity" => Some(Self::LowGravityPowerUp),
            "health_potion" => Some(Self::HealthPotion),
            "cookie" => Some(Self::Cookie),
            _ => None,
        }
    }

    #[must_use]
    pub const fn config_id(self) -> &'static str {
        match self {
            Self::SpeedPowerUp => "speed",
            Self::MultiShotPowerUp => "multi_shot",
            Self::PhasingPowerUp => "phasing",
            Self::LowGravityPowerUp => "low_gravity",
            Self::HealthPotion => "health_potion",
            Self::Cookie => "cookie",
            Self::Key(_) => Self::KEY_CONFIG_ID,
        }
    }
}

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Barrier {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub level: u8,
    pub kind: BarrierKindId,
}

// Visual materials for each segment in the layout. The vectors run parallel
// to `walls` / `ramps` / `floors`: the segment at index `i` renders with the
// `FaceMaterials` at index `i` of the corresponding `*_materials` vector.
// Physics ignores the material vectors.
// Coop puzzle primitive: a floor-cell-mounted plate tagged with a barrier
// kind. While enough plates of a kind are held by players, every barrier of
// that kind becomes fully passable + invisible globally. Distinct from keys
// (per-player filter). Threshold lives on the server; clients just receive
// the set of currently-open kinds via `SSnapshot`.
//
// World-space center is shipped here (not col/row) so the client never needs
// `MapGeometry` to position the visual marker. The server keeps the original
// (col, row) on its own runtime mirror for plate-occupancy tests.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PressurePlate {
    pub level: u8,
    pub center_x: f32,
    pub center_z: f32,
    pub kind: BarrierKindId,
}

// Client-display-only decoration; physics and gameplay ignore it. World-space
// cell center + floor-top y are shipped (not col/row) so the client never
// needs `MapGeometry` to scatter tufts.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct GrassCell {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub level: u8,
}

#[derive(Debug, Clone, Encode, Decode, Resource, Default)]
pub struct MapLayout {
    pub walls: Vec<Wall>,
    pub wall_materials: Vec<FaceMaterials>,
    pub ramps: Vec<Ramp>,
    pub ramp_materials: Vec<FaceMaterials>,
    pub floors: Vec<Floor>,
    pub floor_materials: Vec<FaceMaterials>,
    pub wall_lights: Vec<WallLight>,
    pub barriers: Vec<Barrier>,
    pub pressure_plates: Vec<PressurePlate>,
    pub grass: Vec<GrassCell>,
}

// Per-map tuning defined in `config/server/gameplay.json` under `maps` and
// shipped to clients in `SInit` so prediction uses the server's values.
// Gravity values are positive magnitudes (m/s²); `low_gravity` replaces
// `gravity` while the low-gravity power-up is active.
#[derive(Debug, Clone, Encode, Decode, Resource, serde::Deserialize)]
pub struct MapSettings {
    pub skybox: String,
    pub gravity: f32,
    pub low_gravity: f32,
}

impl MapSettings {
    #[must_use]
    pub fn gravity_for(&self, has_low_gravity: bool) -> f32 {
        if has_low_gravity {
            self.low_gravity
        } else {
            self.gravity
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
            ItemType::PhasingPowerUp,
            ItemType::LowGravityPowerUp,
            ItemType::HealthPotion,
            ItemType::Cookie,
        ];
        for item_type in non_key {
            assert_eq!(ItemType::from_config_id(item_type.config_id()), Some(item_type));
        }
        assert_eq!(ItemType::Key(BarrierKindId(0)).config_id(), ItemType::KEY_CONFIG_ID);
        assert_eq!(ItemType::from_config_id(ItemType::KEY_CONFIG_ID), None);
    }
}
