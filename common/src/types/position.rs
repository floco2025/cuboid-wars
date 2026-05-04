use std::ops::AddAssign;

use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bincode::{Decode, Encode};

// Position component - 3D coordinates in meters (Bevy's coordinate system:
// X, Y=up, Z). Stored as individual fields for serialization.
#[derive(Debug, Clone, Encode, Decode, Copy, Component, PartialEq, Default)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl From<Vec3> for Position {
    fn from(v: Vec3) -> Self {
        Self { x: v.x, y: v.y, z: v.z }
    }
}

impl From<Position> for Vec3 {
    fn from(p: Position) -> Self {
        Self::new(p.x, p.y, p.z)
    }
}

impl AddAssign<Vec3> for Position {
    fn add_assign(&mut self, rhs: Vec3) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}
