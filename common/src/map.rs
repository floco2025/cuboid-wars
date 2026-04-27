use crate::{
    constants::{LEVEL_HEIGHT, PHYSICS_EPSILON, PLAYER_LANDING_EPSILON},
    protocol::{Floor, Ramp},
};

// Calculate the Y position (height) for a given (x, z) position based on ramps.
// Returns the interpolated Y value if the position is on a ramp, otherwise returns 0.0.
#[must_use]
pub fn height_on_ramp(ramps: &[Ramp], x: f32, z: f32) -> f32 {
    ramps
        .iter()
        .find_map(|ramp| {
            let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();

            if x < min_x || x > max_x || z < min_z || z > max_z {
                return None;
            }

            let dx = (ramp.x2 - ramp.x1).abs();
            let dz = (ramp.z2 - ramp.z1).abs();

            let progress = if dx >= dz {
                if (max_x - min_x).abs() < PHYSICS_EPSILON {
                    0.0
                } else {
                    ((x - ramp.x1) / (ramp.x2 - ramp.x1)).clamp(0.0, 1.0)
                }
            } else if (max_z - min_z).abs() < PHYSICS_EPSILON {
                0.0
            } else {
                ((z - ramp.z1) / (ramp.z2 - ramp.z1)).clamp(0.0, 1.0)
            };

            Some(ramp.y1 + progress * (ramp.y2 - ramp.y1))
        })
        .unwrap_or(0.0)
}

// Check if a position (x, z) is currently on any ramp.
#[must_use]
pub fn is_on_ramp(ramps: &[Ramp], x: f32, z: f32) -> bool {
    ramps.iter().any(|ramp| {
        let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();
        x >= min_x && x <= max_x && z >= min_z && z <= max_z
    })
}

// Determine which level a player is on from their Y position. The level
// surface for level k is at `k * LEVEL_HEIGHT`; a player counts as "on level k"
// from `k*LEVEL_HEIGHT - PLAYER_LANDING_EPSILON` up to just below the next
// level's surface, so brief jumps don't change levels. Negative `y` (e.g.
// mid-fall through a ground hole) clamps to level 0.
#[must_use]
pub fn compute_player_level(y: f32) -> u8 {
    if y < -PLAYER_LANDING_EPSILON {
        return 0;
    }
    let raw = ((y + PLAYER_LANDING_EPSILON) / LEVEL_HEIGHT).floor();
    if raw < 0.0 {
        0
    } else {
        raw.min(f32::from(u8::MAX)) as u8
    }
}

// Find the highest supporting surface (ramp or floor) within `±PLAYER_LANDING_EPSILON`
// of the player's feet at (x, z). Returns `None` when the player is in the air with
// no surface within landing range.
#[must_use]
pub fn find_support_floor(floors: &[Floor], ramps: &[Ramp], x: f32, z: f32, y: f32) -> Option<f32> {
    let lo = y - PLAYER_LANDING_EPSILON;
    let hi = y + PLAYER_LANDING_EPSILON;
    let mut best: Option<f32> = None;

    if is_on_ramp(ramps, x, z) {
        let h = height_on_ramp(ramps, x, z);
        if h >= lo && h <= hi {
            best = Some(h);
        }
    }

    for floor in floors {
        if floor.y < lo || floor.y > hi {
            continue;
        }
        let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
        if x < min_x || x > max_x || z < min_z || z > max_z {
            continue;
        }
        best = Some(best.map_or(floor.y, |b| b.max(floor.y)));
    }

    best
}
