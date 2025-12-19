use crate::{
    constants::ROOF_HEIGHT,
    protocol::{Ramp, Roof},
};

// Calculate the Y position (height) for a given (x, z) position based on ramps.
// Returns the interpolated Y value if the position is on a ramp, otherwise returns 0.0.
#[must_use]
pub fn height_on_ramp(ramps: &[Ramp], x: f32, z: f32) -> f32 {
    for ramp in ramps {
        let min_x = ramp.x1.min(ramp.x2);
        let max_x = ramp.x1.max(ramp.x2);
        let min_z = ramp.z1.min(ramp.z2);
        let max_z = ramp.z1.max(ramp.z2);

        if x < min_x || x > max_x || z < min_z || z > max_z {
            continue;
        }

        let dx = (ramp.x2 - ramp.x1).abs();
        let dz = (ramp.z2 - ramp.z1).abs();

        let progress = if dx >= dz {
            if (max_x - min_x).abs() < f32::EPSILON {
                0.0
            } else {
                ((x - ramp.x1) / (ramp.x2 - ramp.x1)).clamp(0.0, 1.0)
            }
        } else if (max_z - min_z).abs() < f32::EPSILON {
            0.0
        } else {
            ((z - ramp.z1) / (ramp.z2 - ramp.z1)).clamp(0.0, 1.0)
        };

        let y = ramp.y1 + progress * (ramp.y2 - ramp.y1);
        return y;
    }

    0.0
}

// Check if a position (x, z) is currently on any ramp.
#[must_use]
pub fn is_on_ramp(ramps: &[Ramp], x: f32, z: f32) -> bool {
    ramps.iter().any(|ramp| {
        let min_x = ramp.x1.min(ramp.x2);
        let max_x = ramp.x1.max(ramp.x2);
        let min_z = ramp.z1.min(ramp.z2);
        let max_z = ramp.z1.max(ramp.z2);

        x >= min_x && x <= max_x && z >= min_z && z <= max_z
    })
}

// Check if a player is on a roof based on their Y position.
#[must_use]
pub fn close_to_roof(y: f32) -> bool {
    const HEIGHT_TOLERANCE: f32 = 0.5;
    (y - ROOF_HEIGHT).abs() < HEIGHT_TOLERANCE
}

// Returns true if the point (x, z) lies within any roof rectangle.
#[must_use]
pub fn has_roof(roofs: &[Roof], x: f32, z: f32) -> bool {
    roofs.iter().any(|roof| {
        let min_x = roof.x1.min(roof.x2);
        let max_x = roof.x1.max(roof.x2);
        let min_z = roof.z1.min(roof.z2);
        let max_z = roof.z1.max(roof.z2);

        x >= min_x && x <= max_x && z >= min_z && z <= max_z
    })
}
