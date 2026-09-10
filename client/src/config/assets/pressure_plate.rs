use anyhow::{Result, ensure};
use common::protocol::HexColor;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PressurePlateDef {
    pub scene: String,
    pub default_color: HexColor,
    pub light_color: [f32; 3],
    pub emissive_luminance: f32,
}

impl PressurePlateDef {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(!self.scene.trim().is_empty(), "pressure_plate.scene must not be empty");
        ensure!(
            self.light_color
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value)),
            "pressure_plate.light_color must contain RGB values from 0 to 1"
        );
        ensure!(
            self.emissive_luminance.is_finite() && self.emissive_luminance >= 0.0,
            "pressure_plate.emissive_luminance must be nonnegative"
        );
        Ok(())
    }
}
