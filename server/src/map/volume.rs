use bevy::prelude::Vec3;
use common::map::MapGeometry;

#[derive(Clone, Copy, Debug)]
pub struct ZoneVolume {
    pub min: Vec3,
    pub max: Vec3,
}

impl ZoneVolume {
    pub fn from_grid(geometry: MapGeometry, level: u8, levels: u16, cols: [i32; 2], rows: [i32; 2]) -> Self {
        Self {
            min: Vec3::new(
                geometry.cell_to_world_x(cols[0]),
                geometry.level_y(level),
                geometry.cell_to_world_z(rows[0]),
            ),
            max: Vec3::new(
                geometry.cell_to_world_x(cols[1]),
                geometry.level_y(level) + f32::from(levels) * geometry.level_height(),
                geometry.cell_to_world_z(rows[1]),
            ),
        }
    }

    // Distance to the extended volume summed over the horizontal axes, zero
    // inside it.
    pub fn axis_distance_outside(self, point: Vec3, extension: f32) -> f32 {
        let offset = point - point.clamp(self.min, self.max);
        (offset.x.abs() + offset.z.abs() - extension).max(0.0)
    }

    pub fn contains(self, point: Vec3, extension: f32) -> bool {
        point.is_finite()
            && point
                .as_dvec3()
                .distance_squared(point.clamp(self.min, self.max).as_dvec3())
                <= f64::from(extension).powi(2) + 0.000001
    }
}

#[cfg(test)]
#[path = "tests/volume.rs"]
mod tests;
