use crate::{
    constants::{LEVEL_CLASSIFICATION_TOLERANCE, LEVEL_HEIGHT, PHYSICS_EPSILON},
    protocol::{Floor, Ramp},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RampAxis {
    X,
    Z,
}

#[must_use]
pub fn ramp_axis(ramp: &Ramp) -> RampAxis {
    if (ramp.x2 - ramp.x1).abs() >= (ramp.z2 - ramp.z1).abs() {
        RampAxis::X
    } else {
        RampAxis::Z
    }
}

#[must_use]
pub fn ramp_slope(ramp: &Ramp) -> f32 {
    let denom = match ramp_axis(ramp) {
        RampAxis::X => ramp.x2 - ramp.x1,
        RampAxis::Z => ramp.z2 - ramp.z1,
    };
    if denom.abs() < PHYSICS_EPSILON {
        0.0
    } else {
        (ramp.y2 - ramp.y1) / denom
    }
}

// Compute the surface Y of a single ramp at (x, z). Caller must have already
// verified that (x, z) lies inside `ramp.bounds_xz()`.
#[must_use]
pub fn ramp_surface_at(ramp: &Ramp, x: f32, z: f32) -> f32 {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();

    let progress = match ramp_axis(ramp) {
        RampAxis::X if (max_x - min_x).abs() >= PHYSICS_EPSILON => {
            ((x - ramp.x1) / (ramp.x2 - ramp.x1)).clamp(0.0, 1.0)
        }
        RampAxis::Z if (max_z - min_z).abs() >= PHYSICS_EPSILON => {
            ((z - ramp.z1) / (ramp.z2 - ramp.z1)).clamp(0.0, 1.0)
        }
        _ => 0.0,
    };

    ramp.y1 + progress * (ramp.y2 - ramp.y1)
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
// from `k*LEVEL_HEIGHT - LEVEL_CLASSIFICATION_TOLERANCE` up to just below the next
// level's surface, so brief jumps don't change levels. Negative `y` (e.g.
// mid-fall through a ground hole) clamps to level 0.
#[must_use]
pub fn compute_player_level(y: f32) -> u8 {
    if y < -LEVEL_CLASSIFICATION_TOLERANCE {
        return 0;
    }
    let raw = ((y + LEVEL_CLASSIFICATION_TOLERANCE) / LEVEL_HEIGHT).floor();
    if raw < 0.0 {
        0
    } else {
        raw.min(f32::from(u8::MAX)) as u8
    }
}

// Find the highest supporting surface (ramp or floor) within `±LEVEL_CLASSIFICATION_TOLERANCE`
// of the player's feet at (x, z). Returns `None` when the player is in the air with
// no surface within landing range.
//
// Iterates *all* ramps containing (x, z) — when ramps stack at the same XZ
// (e.g. the basement→lobby ramp and the rooms-low→rooms-high ramp share a
// stairshaft column), only the one whose surface is in landing range
// counts.
#[must_use]
pub fn find_support_floor(floors: &[Floor], ramps: &[Ramp], x: f32, z: f32, y: f32) -> Option<f32> {
    let lo = y - LEVEL_CLASSIFICATION_TOLERANCE;
    let hi = y + LEVEL_CLASSIFICATION_TOLERANCE;
    let mut best: Option<f32> = None;

    for ramp in ramps {
        let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();
        if x < min_x || x > max_x || z < min_z || z > max_z {
            continue;
        }
        let h = ramp_surface_at(ramp, x, z);
        if h >= lo && h <= hi {
            best = Some(best.map_or(h, |b| b.max(h)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::LEVEL_HEIGHT;

    #[test]
    fn ramp_axis_and_surface_follow_longer_x_axis() {
        let ramp = Ramp {
            x1: -4.0,
            y1: 0.0,
            z1: 0.0,
            x2: 4.0,
            y2: LEVEL_HEIGHT,
            z2: 2.0,
        };

        assert_eq!(ramp_axis(&ramp), RampAxis::X);
        assert!((ramp_slope(&ramp) - LEVEL_HEIGHT / 8.0).abs() < PHYSICS_EPSILON);
        assert!((ramp_surface_at(&ramp, 0.0, 0.0) - LEVEL_HEIGHT / 2.0).abs() < PHYSICS_EPSILON);
    }

    #[test]
    fn ramp_axis_and_surface_follow_longer_z_axis() {
        let ramp = Ramp {
            x1: 0.0,
            y1: 0.0,
            z1: -4.0,
            x2: 2.0,
            y2: LEVEL_HEIGHT,
            z2: 4.0,
        };

        assert_eq!(ramp_axis(&ramp), RampAxis::Z);
        assert!((ramp_slope(&ramp) - LEVEL_HEIGHT / 8.0).abs() < PHYSICS_EPSILON);
        assert!((ramp_surface_at(&ramp, 0.0, 0.0) - LEVEL_HEIGHT / 2.0).abs() < PHYSICS_EPSILON);
    }
}
