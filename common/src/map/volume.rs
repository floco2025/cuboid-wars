use super::MapGeometry;
use bevy_math::Vec3;

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

    pub fn distance_squared(self, point: Vec3) -> f32 {
        point.distance_squared(point.clamp(self.min, self.max))
    }

    pub fn contains(self, point: Vec3, extension: f32) -> bool {
        self.distance_squared(point) <= extension * extension + 0.000001
    }
}

#[cfg(test)]
#[path = "tests/volume.rs"]
mod tests;
