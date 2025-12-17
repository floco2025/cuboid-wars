use crate::protocol::Ramp;

/// Calculate the Y position (height) for a given (x, z) position based on ramps.
/// Returns the interpolated Y value if the position is on a ramp, otherwise returns 0.0.
#[must_use]
pub fn calculate_height_at_position(ramps: &[Ramp], x: f32, z: f32) -> f32 {
    for ramp in ramps {
        // Determine ramp footprint bounds
        let min_x = ramp.x1.min(ramp.x2);
        let max_x = ramp.x1.max(ramp.x2);
        let min_z = ramp.z1.min(ramp.z2);
        let max_z = ramp.z1.max(ramp.z2);

        // Check if position is within ramp footprint
        if x < min_x || x > max_x || z < min_z || z > max_z {
            continue;
        }

        // Position is on this ramp - calculate interpolated height
        // Determine if ramp is primarily along X or Z axis
        let dx = (ramp.x2 - ramp.x1).abs();
        let dz = (ramp.z2 - ramp.z1).abs();

        let progress = if dx >= dz {
            // Ramp along X axis
            // x1 is at y1, x2 is at y2
            if (max_x - min_x).abs() < f32::EPSILON {
                0.0
            } else {
                ((x - ramp.x1) / (ramp.x2 - ramp.x1)).clamp(0.0, 1.0)
            }
        } else {
            // Ramp along Z axis
            // z1 is at y1, z2 is at y2
            if (max_z - min_z).abs() < f32::EPSILON {
                0.0
            } else {
                ((z - ramp.z1) / (ramp.z2 - ramp.z1)).clamp(0.0, 1.0)
            }
        };

        // Linear interpolation between y1 and y2
        let y = ramp.y1 + progress * (ramp.y2 - ramp.y1);
        return y;
    }

    // Not on any ramp
    0.0
}
