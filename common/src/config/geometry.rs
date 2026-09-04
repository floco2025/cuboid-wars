use anyhow::{Result, ensure};
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::validation::validate_positive_finite;
use crate::constants::{BARRIER_THICKNESS_FRACTION, BRIDGE_THICKNESS_FRACTION, LEVEL_CLASSIFICATION_TOLERANCE};

// The sizes every other map measure follows: the edge of one grid cell, the
// storey pitch, and the slab and wall thicknesses. Per map in
// `gameplay.json`, shipped to clients inside the map settings; the derived
// sizes below are the only other way to obtain a world dimension.
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode, Deserialize)]
pub struct MapGeometryConfig {
    pub grid_cell_size: f32,
    pub level_height: f32,
    pub floor_thickness: f32,
    pub wall_thickness: f32,
}

impl MapGeometryConfig {
    pub fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.grid_cell_size, &format!("{path}.grid_cell_size"))?;
        validate_positive_finite(self.level_height, &format!("{path}.level_height"))?;
        validate_positive_finite(self.floor_thickness, &format!("{path}.floor_thickness"))?;
        validate_positive_finite(self.wall_thickness, &format!("{path}.wall_thickness"))?;
        ensure!(
            self.floor_thickness < self.level_height,
            "{path}.floor_thickness must be less than {path}.level_height (the wall would have no height)"
        );
        Ok(())
    }

    // A storey's wall: the pitch minus the slab it carries.
    #[must_use]
    pub fn wall_height(&self) -> f32 {
        self.level_height - self.floor_thickness
    }

    #[must_use]
    pub fn wall_half_thickness(&self) -> f32 {
        self.wall_thickness / 2.0
    }

    #[must_use]
    pub fn barrier_thickness(&self) -> f32 {
        self.wall_thickness * BARRIER_THICKNESS_FRACTION
    }

    #[must_use]
    pub fn bridge_thickness(&self) -> f32 {
        self.floor_thickness * BRIDGE_THICKNESS_FRACTION
    }

    // The floor surface of storey `level`.
    #[must_use]
    pub fn level_y(&self, level: u8) -> f32 {
        f32::from(level) * self.level_height
    }

    // The storey a body at `y` is on. A body counts as on level k from
    // `LEVEL_CLASSIFICATION_TOLERANCE` below its surface up to just below the
    // next surface, so brief jumps don't change levels; below level 0 (a
    // fall through a ground hole) clamps to 0.
    #[must_use]
    pub fn level_for_y(&self, y: f32) -> u8 {
        if y < -LEVEL_CLASSIFICATION_TOLERANCE {
            return 0;
        }
        let raw = ((y + LEVEL_CLASSIFICATION_TOLERANCE) / self.level_height).floor();
        if raw < 0.0 {
            0
        } else {
            raw.min(f32::from(u8::MAX)) as u8
        }
    }

    // The storey whose surface is nearest to `y`, for geometry that sits on a
    // surface rather than a body that may be mid-jump.
    #[must_use]
    pub fn nearest_level_to_y(&self, y: f32) -> u8 {
        (y / self.level_height).round().clamp(0.0, f32::from(u8::MAX)) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, sizes};

    #[test]
    fn wall_height_is_the_pitch_minus_the_slab() {
        assert_eq!(sizes().wall_height(), WALL_HEIGHT);
        assert_eq!(sizes().wall_height() + FLOOR_THICKNESS, LEVEL_HEIGHT);
    }

    #[test]
    fn level_for_y_tolerates_a_small_dip_and_clamps_below_ground() {
        let sizes = sizes();
        assert_eq!(sizes.level_for_y(0.0), 0);
        assert_eq!(sizes.level_for_y(-0.4), 0);
        assert_eq!(sizes.level_for_y(-10.0), 0);
        assert_eq!(sizes.level_for_y(LEVEL_HEIGHT - 0.2), 1);
        assert_eq!(sizes.level_for_y(LEVEL_HEIGHT - 0.6), 0);
        assert_eq!(sizes.level_for_y(2.0 * LEVEL_HEIGHT + 1.0), 2);
    }

    #[test]
    fn nearest_level_rounds_to_the_closest_surface() {
        let sizes = sizes();
        assert_eq!(sizes.nearest_level_to_y(0.4 * LEVEL_HEIGHT), 0);
        assert_eq!(sizes.nearest_level_to_y(0.6 * LEVEL_HEIGHT), 1);
        assert_eq!(sizes.nearest_level_to_y(-3.0), 0);
    }

    #[test]
    fn validation_names_the_bad_field() {
        let zero_cell = MapGeometryConfig {
            grid_cell_size: 0.0,
            ..sizes()
        };
        let error = zero_cell
            .validate("maps.hotel.geometry")
            .expect_err("zero cell size accepted");
        assert!(
            error.to_string().contains("maps.hotel.geometry.grid_cell_size"),
            "{error}"
        );

        let no_wall = MapGeometryConfig {
            floor_thickness: LEVEL_HEIGHT,
            ..sizes()
        };
        let error = no_wall
            .validate("maps.hotel.geometry")
            .expect_err("slab as thick as the storey accepted");
        assert!(error.to_string().contains("floor_thickness"), "{error}");
    }
}
