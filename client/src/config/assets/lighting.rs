use anyhow::{Result, ensure};
use bevy::prelude::Vec3;
use common::config::{validate_non_negative_finite, validate_positive_finite};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct WallLightModelDef {
    pub color: [f32; 3],
    pub flicker: bool,
    pub scene: String,
    pub scale: f32,
    pub offset_from_wall: f32,
    pub brightness: f32,
    pub range: f32,
    pub radius: f32,
    pub emissive_luminance: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkyboxDef {
    // Path to a cube-cross layout image used to derive the cubemap faces.
    pub image: String,
    pub brightness: f32,
    // Seconds per full ambient sky turn; 0 (or absent) = static sky.
    #[serde(default)]
    pub rotation_period_secs: f32,
    // Light rotation advances in discrete steps of this size so shadow maps
    // stay pixel-stable between steps; 0 (or absent) = continuous (shadow
    // edges shimmer while the sun creeps).
    #[serde(default)]
    pub celestial_step_degrees: f32,
    pub celestial_disc: CelestialDiscDef,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CelestialDiscDef {
    pub direction: [f32; 3],
    pub show: bool,
    pub distance: f32,
    pub radius: f32,
    pub bright: CelestialDiscLook,
    pub dim: CelestialDiscLook,
    pub dark: CelestialDiscLook,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CelestialDiscLook {
    pub luminance: f32,
    pub phase_percent: f32,
}

impl SkyboxDef {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        ensure!(!self.image.trim().is_empty(), "{path}.image must not be empty");
        validate_non_negative_finite(self.brightness, &format!("{path}.brightness"))?;
        validate_non_negative_finite(self.rotation_period_secs, &format!("{path}.rotation_period_secs"))?;
        validate_non_negative_finite(self.celestial_step_degrees, &format!("{path}.celestial_step_degrees"))?;
        ensure!(
            self.celestial_step_degrees < 360.0,
            "{path}.celestial_step_degrees must be less than 360"
        );
        ensure!(
            Vec3::from_array(self.celestial_disc.direction)
                .try_normalize()
                .is_some(),
            "{path}.celestial_disc.direction must be a finite, nonzero vector"
        );
        validate_positive_finite(self.celestial_disc.distance, &format!("{path}.celestial_disc.distance"))?;
        validate_positive_finite(self.celestial_disc.radius, &format!("{path}.celestial_disc.radius"))?;
        for (name, look) in [
            ("bright", self.celestial_disc.bright),
            ("dim", self.celestial_disc.dim),
            ("dark", self.celestial_disc.dark),
        ] {
            let path = format!("{path}.celestial_disc.{name}");
            validate_non_negative_finite(look.luminance, &format!("{path}.luminance"))?;
            ensure!(
                (0.0..=100.0).contains(&look.phase_percent),
                "{path}.phase_percent must be in [0, 100]"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/lighting.rs"]
mod tests;
