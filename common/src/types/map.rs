use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};

use super::Position;
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
    Cookie,
}

// Visual materials for each segment in the layout. The vectors run parallel
// to `walls` / `ramps` / `floors`: the segment at index `i` renders with the
// `FaceMaterials` at index `i` of the corresponding `*_materials` vector.
// Physics ignores the material vectors.
#[derive(Debug, Clone, Encode, Decode, Resource, Default)]
pub struct MapLayout {
    pub walls: Vec<Wall>,
    pub wall_materials: Vec<FaceMaterials>,
    pub ramps: Vec<Ramp>,
    pub ramp_materials: Vec<FaceMaterials>,
    pub floors: Vec<Floor>,
    pub floor_materials: Vec<FaceMaterials>,
    pub wall_lights: Vec<WallLight>,
}
