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
    AntiGravityPowerUp,
    // Instant heal on pickup; no durable state on `PlayerInfo` (unlike the
    // other power-ups, which arm a timer). The heal amount comes from
    // `PowerUpsConfig.health_potion_heal_percent`.
    HealthPotion,
    Cookie,
    // Key, parameterized by the barrier kind it eventually unlocks. World-
    // spawned via `KeySpawnZone`s; once collected, the kind enters the
    // player's permanent inventory.
    Key(BarrierKindId),
}

impl ItemType {
    // Items that arm a per-player timer on pickup (the classic four
    // power-ups). `HealthPotion` is NOT one of these — its effect is
    // instant; see `PowerUpKind`.
    #[must_use]
    pub const fn is_timer_power_up(self) -> bool {
        matches!(
            self,
            Self::SpeedPowerUp | Self::MultiShotPowerUp | Self::PhasingPowerUp | Self::AntiGravityPowerUp
        )
    }

    // Items that use the spawn-time countdown for re-show after collection
    // (cookies + keys), versus items that despawn the world entity entirely
    // on pickup (power-ups + health potion). The dispatch in
    // `item_collection_system` reads this to gate "currently respawning"
    // visibility.
    #[must_use]
    pub const fn respects_respawn_timer(self) -> bool {
        matches!(self, Self::Cookie | Self::Key(_))
    }
}

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Barrier {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub width: f32,
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
}
