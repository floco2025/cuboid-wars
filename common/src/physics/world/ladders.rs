use bevy_math::Vec3;

use crate::{
    constants::{
        LADDER_BAND_DEPTH, LADDER_BASE_OVERSHOOT, LADDER_OVERSHOOT, LADDER_RAIL_INSET, LADDER_VOLUME_DEPTH,
        PHYSICS_EPSILON,
    },
    protocol::{Ladder, Position},
};

// Climb volume and blocking band derived from a `Ladder`. Deliberately not a
// Rapier collider: the character step queries these boxes directly, so
// ladders cost no collision-group bit and stay invisible to projectiles and
// character filters.
//
// Ladders are one-sided. The FRONT is the rail side — where the normal
// points and the client draws the rails — and the climb volume covers only
// it, so a back-side character is simply not on a ladder: no ride, no
// latch, and (via the fence's own side check) no barrier — they pass
// through the plane and emerge on the front face. That one-way membrane is
// what lets a walker mount mid-ladder from a balcony behind it.
#[derive(Debug, Clone, Copy)]
pub struct LadderVolume {
    // Climbable region, in front of the rail plane only, overshooting both
    // ends: above the top landing for the crest, below the base so the
    // bottom of the ladder can be grabbed.
    min: Vec3,
    max: Vec3,
    // Blocking band: both sides of the plane (move targets on either side
    // must be caught) and wider than the climb volume — it must cover any
    // character's hold distance — but capped at the top landing's surface.
    // Below that height the plane is a fence for front-side characters; at
    // or above it the plane is open, which is what lets a climb crest over
    // the top and a character on the top landing step onto the ladder. It
    // reaches down to the climb volume's bottom: a climb starting from the
    // last-rung hang is held at the plane only by this fence.
    band_min: Vec3,
    band_max: Vec3,
    // Unit axis-aligned normal of the edge plane, pointing at the front.
    pub normal_x: f32,
    pub normal_z: f32,
    // A point on the rail plane (the segment midpoint).
    mid_x: f32,
    mid_z: f32,
}

impl LadderVolume {
    #[must_use]
    pub fn from_ladder(ladder: &Ladder) -> Self {
        let y_min = ladder.y;
        let top_landing = ladder.y + ladder.height;
        // Everything is measured from the RAIL plane — where the client
        // draws the rails — not the anchoring grid edge, so the physics
        // uses the ladder where it visibly is.
        let rail_x = ladder.nx * LADDER_RAIL_INSET;
        let rail_z = ladder.nz * LADDER_RAIL_INSET;
        let x_near = ladder.x1.min(ladder.x2) + rail_x;
        let x_far = ladder.x1.max(ladder.x2) + rail_x;
        let z_near = ladder.z1.min(ladder.z2) + rail_z;
        let z_far = ladder.z1.max(ladder.z2) + rail_z;
        // The climb volume reaches `LADDER_VOLUME_DEPTH` from the plane on
        // the front side only; the band reaches `LADDER_BAND_DEPTH` on both.
        let front_x = ladder.nx * LADDER_VOLUME_DEPTH;
        let front_z = ladder.nz * LADDER_VOLUME_DEPTH;
        let band_x = (ladder.nx * LADDER_BAND_DEPTH).abs();
        let band_z = (ladder.nz * LADDER_BAND_DEPTH).abs();
        Self {
            min: Vec3::new(
                x_near + front_x.min(0.0),
                y_min - LADDER_BASE_OVERSHOOT,
                z_near + front_z.min(0.0),
            ),
            max: Vec3::new(
                x_far + front_x.max(0.0),
                top_landing + LADDER_OVERSHOOT,
                z_far + front_z.max(0.0),
            ),
            band_min: Vec3::new(x_near - band_x, y_min - LADDER_BASE_OVERSHOOT, z_near - band_z),
            band_max: Vec3::new(x_far + band_x, top_landing, z_far + band_z),
            normal_x: ladder.nx,
            normal_z: ladder.nz,
            mid_x: f32::midpoint(ladder.x1, ladder.x2) + rail_x,
            mid_z: f32::midpoint(ladder.z1, ladder.z2) + rail_z,
        }
    }

    // The bottom is tolerant: a descent clamps to `min.y` but the resolved
    // move lands a rounding error below it, and an exact check would drop
    // the character off the last rung.
    #[must_use]
    pub fn contains(&self, pos: &Position) -> bool {
        pos.x >= self.min.x
            && pos.x <= self.max.x
            && pos.y >= self.min.y - PHYSICS_EPSILON
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

    // Lowest point the climb volume reaches (`LADDER_BASE_OVERSHOOT` below
    // the base) — where a descent stops and hangs.
    #[must_use]
    pub fn bottom_y(&self) -> f32 {
        self.min.y
    }

    // Perpendicular offset of (x, z) from the rail plane along the normal:
    // positive in front of the ladder, negative behind it.
    #[must_use]
    pub fn offset_from_plane(&self, x: f32, z: f32) -> f32 {
        self.normal_x * (x - self.mid_x) + self.normal_z * (z - self.mid_z)
    }

    // Horizontal offset from the ladder's vertical center axis, projected
    // along its face so centering never changes the rail-plane standoff.
    #[must_use]
    pub fn offset_from_axis(&self, x: f32, z: f32) -> Vec3 {
        let tangent = Vec3::new(-self.normal_z, 0.0, self.normal_x);
        tangent * Vec3::new(x - self.mid_x, 0.0, z - self.mid_z).dot(tangent)
    }

    // (x, z) shifted along the normal so its plane offset becomes `offset`.
    #[must_use]
    pub fn with_plane_offset(&self, x: f32, z: f32, offset: f32) -> (f32, f32) {
        let shift = offset - self.offset_from_plane(x, z);
        (self.normal_x.mul_add(shift, x), self.normal_z.mul_add(shift, z))
    }
}
