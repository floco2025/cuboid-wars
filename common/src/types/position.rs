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

impl Position {
    // Squared XZ-plane distance (ignores Y). Squared to avoid the sqrt at
    // comparison sites.
    #[must_use]
    pub fn horizontal_distance_sq(&self, other: &Self) -> f32 {
        let dx = self.x - other.x;
        let dz = self.z - other.z;
        dx.mul_add(dx, dz * dz)
    }

    // Squared 3D distance.
    #[must_use]
    pub fn distance_sq(&self, other: &Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx.mul_add(dx, dy.mul_add(dy, dz * dz))
    }
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
