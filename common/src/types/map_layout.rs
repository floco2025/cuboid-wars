use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};

use super::face_materials::FaceMaterials;
use super::{BarrierKindId, Position};

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Wall {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub width: f32,
    pub level: u8,
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

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Barrier {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub level: u8,
    pub kind: BarrierKindId,
}

// Freestanding climbable element anchored on a grid edge. The segment is the
// edge span already shrunk to `LADDER_WIDTH` centered on the edge midpoint.
// One-sided: the normal points at the FRONT — the climbable rail side —
// while the back is passed through. No Rapier collider — the character step
// queries the derived climb volumes directly.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Ladder {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub nx: f32,
    pub nz: f32,
    pub level: u8,
    pub levels: u8,
}

// Visual materials for each segment in the layout. The vectors run parallel
// to `walls` / `ramps` / `floors`: the segment at index `i` renders with the
// `FaceMaterials` at index `i` of the corresponding `*_materials` vector.
// Physics ignores the material vectors.
// What holding a plate does. Barrier plates open every barrier of their kind
// (fully passable + invisible, globally) while enough of them are held —
// distinct from keys (per-player filter). Firework plates launch the show
// once enough players stand on them. Thresholds live on the server; clients
// receive the open kinds via `SSnapshot` and the show via `SFirework`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Encode, Decode)]
pub enum PlatePurpose {
    Barrier(BarrierKindId),
    Firework,
}

// Coop puzzle primitive: a floor-cell-mounted plate. World-space center is
// shipped here (not col/row) so the client never needs `MapGeometry` to
// position the visual marker. The server keeps the original (col, row) on
// its own runtime mirror for plate-occupancy tests.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PressurePlate {
    pub level: u8,
    pub center_x: f32,
    pub center_z: f32,
    pub purpose: PlatePurpose,
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
    pub ladders: Vec<Ladder>,
    pub pressure_plates: Vec<PressurePlate>,
    pub grass: Vec<GrassCell>,
}

impl MapLayout {
    // One-line element tally for the server's generate log and the
    // client's spawn log, so both report the same things in the same order.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} walls, {} floors, {} ramps, {} ladders, {} barriers, {} wall lights, {} pressure plates",
            self.walls.len(),
            self.floors.len(),
            self.ramps.len(),
            self.ladders.len(),
            self.barriers.len(),
            self.wall_lights.len(),
            self.pressure_plates.len(),
        )
    }
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
