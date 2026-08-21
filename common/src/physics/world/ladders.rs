use bevy_math::Vec3;

use crate::{
    constants::{LADDER_BAND_DEPTH, LADDER_OVERSHOOT, LADDER_RAIL_INSET, LADDER_VOLUME_DEPTH, LEVEL_HEIGHT},
    protocol::{Ladder, Position},
};

// Axis-aligned climb volume derived from a `Ladder`. Deliberately not a
// Rapier collider: the character step queries these volumes directly, so
// ladders cost no collision-group bit and stay invisible to projectiles and
// character filters. Both structures are symmetric about the edge plane —
// a ladder is climbable from either side; whatever caps one side is an
// ordinary collision, not a ladder rule.
#[derive(Debug, Clone, Copy)]
pub struct LadderVolume {
    // Climbable region: both sides of the plane, up to the overshoot above
    // the top landing.
    min: Vec3,
    max: Vec3,
    // Blocking band: wider than the climb volume (it must cover any
    // character's hold distance) but capped at the top landing's surface —
    // below that height the plane is a fence, at or above it the plane is
    // open, which is what lets a climb crest over the top and a character on
    // the top landing step onto the ladder.
    band_min: Vec3,
    band_max: Vec3,
    // Unit axis-aligned normal of the edge plane. Only orients the two
    // boxes and the client's rail mesh; climbing is side-agnostic.
    pub normal_x: f32,
    pub normal_z: f32,
    // A point on the edge plane (the segment midpoint).
    mid_x: f32,
    mid_z: f32,
}

impl LadderVolume {
    #[must_use]
    pub fn from_ladder(ladder: &Ladder) -> Self {
        let y_min = f32::from(ladder.level) * LEVEL_HEIGHT;
        let top_landing = (f32::from(ladder.level) + f32::from(ladder.levels)) * LEVEL_HEIGHT;
        // Everything is centered on the RAIL plane — where the client draws
        // the rails — not on the anchoring grid edge, so the physics uses
        // the ladder where it visibly is.
        let rail_x = ladder.nx * LADDER_RAIL_INSET;
        let rail_z = ladder.nz * LADDER_RAIL_INSET;
        let x_near = ladder.x1.min(ladder.x2) + rail_x;
        let x_far = ladder.x1.max(ladder.x2) + rail_x;
        let z_near = ladder.z1.min(ladder.z2) + rail_z;
        let z_far = ladder.z1.max(ladder.z2) + rail_z;
        let volume_x = (ladder.nx * LADDER_VOLUME_DEPTH).abs();
        let volume_z = (ladder.nz * LADDER_VOLUME_DEPTH).abs();
        let band_x = (ladder.nx * LADDER_BAND_DEPTH).abs();
        let band_z = (ladder.nz * LADDER_BAND_DEPTH).abs();
        Self {
            min: Vec3::new(x_near - volume_x, y_min, z_near - volume_z),
            max: Vec3::new(x_far + volume_x, top_landing + LADDER_OVERSHOOT, z_far + volume_z),
            band_min: Vec3::new(x_near - band_x, y_min, z_near - band_z),
            band_max: Vec3::new(x_far + band_x, top_landing, z_far + band_z),
            normal_x: ladder.nx,
            normal_z: ladder.nz,
            mid_x: f32::midpoint(ladder.x1, ladder.x2) + rail_x,
            mid_z: f32::midpoint(ladder.z1, ladder.z2) + rail_z,
        }
    }

    #[must_use]
    pub fn contains(&self, pos: &Position) -> bool {
        pos.x >= self.min.x
            && pos.x <= self.max.x
            && pos.y >= self.min.y
            && pos.y <= self.max.y
            && pos.z >= self.min.z
            && pos.z <= self.max.z
    }

    // The top is exclusive: feet level with the top landing are already
    // clear of the fence.
    #[must_use]
    pub fn band_contains(&self, x: f32, z: f32, y: f32) -> bool {
        x >= self.band_min.x
            && x <= self.band_max.x
            && y >= self.band_min.y
            && y < self.band_max.y
            && z >= self.band_min.z
            && z <= self.band_max.z
    }

    // Y of the top landing's walking surface (the climb volume extends
    // `LADDER_OVERSHOOT` above it; the blocking band stops at it).
    #[must_use]
    pub fn top_landing_y(&self) -> f32 {
        self.band_max.y
    }

    // Which side of the plane (x, z) is on: +1.0 or -1.0 along the normal.
    #[must_use]
    pub fn side_sign(&self, x: f32, z: f32) -> f32 {
        if self.offset_from_plane(x, z) < 0.0 { -1.0 } else { 1.0 }
    }

    // Perpendicular offset of (x, z) from the edge plane along the normal.
    #[must_use]
    pub fn offset_from_plane(&self, x: f32, z: f32) -> f32 {
        self.normal_x * (x - self.mid_x) + self.normal_z * (z - self.mid_z)
    }

    // (x, z) shifted along the normal so its plane offset becomes `offset`.
    #[must_use]
    pub fn with_plane_offset(&self, x: f32, z: f32, offset: f32) -> (f32, f32) {
        let shift = offset - self.offset_from_plane(x, z);
        (self.normal_x.mul_add(shift, x), self.normal_z.mul_add(shift, z))
    }
}
