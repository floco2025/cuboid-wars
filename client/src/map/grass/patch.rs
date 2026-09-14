use common::protocol::{CarrierId, Floor};

use crate::constants::GRASS_CHUNK_SIZE;

#[derive(Clone, Copy, Debug)]
pub(in crate::map) struct GrassPatch {
    pub(in crate::map) x1: f32,
    pub(in crate::map) x2: f32,
    pub(in crate::map) z1: f32,
    pub(in crate::map) z2: f32,
    pub(in crate::map) y: f32,
    pub(in crate::map) level: u8,
    pub(in crate::map) carrier: CarrierId,
}

impl GrassPatch {
    pub(in crate::map) fn clipped_to_chunk(floor: Floor, chunk_x: i32, chunk_z: i32) -> Option<Self> {
        let (floor_x1, floor_x2, floor_z1, floor_z2) = floor.bounds_xz();
        let chunk_x1 = chunk_x as f32 * GRASS_CHUNK_SIZE;
        let chunk_z1 = chunk_z as f32 * GRASS_CHUNK_SIZE;
        let x1 = floor_x1.max(chunk_x1);
        let x2 = floor_x2.min(chunk_x1 + GRASS_CHUNK_SIZE);
        let z1 = floor_z1.max(chunk_z1);
        let z2 = floor_z2.min(chunk_z1 + GRASS_CHUNK_SIZE);
        (x2 > x1 && z2 > z1).then_some(Self {
            x1,
            x2,
            z1,
            z2,
            y: floor.y,
            level: floor.level,
            carrier: floor.carrier,
        })
    }

    pub(super) fn area(self) -> f32 {
        (self.x2 - self.x1) * (self.z2 - self.z1)
    }
}
