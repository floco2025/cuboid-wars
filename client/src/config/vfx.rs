use anyhow::{Result, bail};
use serde::Deserialize;

use super::settings::{validate_non_negative_finite, validate_positive_finite, validate_unit_ratio};

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct VfxConfig {
    pub pickups: PickupVfxConfig,
    pub fields: FieldVfxConfig,
    pub erasers: EraserVfxConfig,
    pub checkpoints: CheckpointVfxConfig,
}

impl VfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
        self.pickups.validate()?;
        self.fields.validate()?;
        self.erasers.validate()?;
        self.checkpoints.validate()
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PickupVfxConfig {
    pub emissive_brightness: f32,
}

impl PickupVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.emissive_brightness, "vfx.pickups.emissive_brightness")
    }
}

// The one look of barriers and light bridges: a field that is on shows at
// `opacity`, one that is off at `passable_opacity`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FieldVfxConfig {
    pub emissive_brightness: f32,
    pub rail_emissive_brightness: f32,
    pub opacity: f32,
    pub passable_opacity: f32,
    pub fade_secs: f32,
}

impl FieldVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.emissive_brightness, "vfx.fields.emissive_brightness")?;
        validate_non_negative_finite(self.rail_emissive_brightness, "vfx.fields.rail_emissive_brightness")?;
        validate_unit_ratio(self.opacity, "vfx.fields.opacity")?;
        validate_unit_ratio(self.passable_opacity, "vfx.fields.passable_opacity")?;
        validate_positive_finite(self.fade_secs, "vfx.fields.fade_secs")?;
        if self.passable_opacity > self.opacity {
            bail!("vfx.fields.passable_opacity must not exceed vfx.fields.opacity");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct EraserVfxConfig {
    pub emissive_brightness: f32,
    pub rail_emissive_brightness: f32,
    pub opacity: f32,
}

impl EraserVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.emissive_brightness, "vfx.erasers.emissive_brightness")?;
        validate_non_negative_finite(self.rail_emissive_brightness, "vfx.erasers.rail_emissive_brightness")?;
        validate_unit_ratio(self.opacity, "vfx.erasers.opacity")
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CheckpointVfxConfig {
    pub flag_emissive_brightness: f32,
    pub paint_emissive_brightness: f32,
}

impl CheckpointVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(
            self.flag_emissive_brightness,
            "vfx.checkpoints.flag_emissive_brightness",
        )?;
        validate_non_negative_finite(
            self.paint_emissive_brightness,
            "vfx.checkpoints.paint_emissive_brightness",
        )
    }
}

#[cfg(test)]
#[path = "tests/vfx.rs"]
mod tests;
