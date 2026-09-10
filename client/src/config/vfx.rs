use anyhow::{Result, bail};
use serde::Deserialize;

use super::settings::{validate_non_negative_finite, validate_positive_finite, validate_unit_ratio};

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct VfxConfig {
    pub pickups: PickupVfxConfig,
    pub barriers: BarrierVfxConfig,
    pub erasers: EraserVfxConfig,
    pub light_bridges: LightBridgeVfxConfig,
}

impl VfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
        self.pickups.validate()?;
        self.barriers.validate()?;
        self.erasers.validate()?;
        self.light_bridges.validate()
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

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BarrierVfxConfig {
    pub emissive_brightness: f32,
    pub opacity: f32,
    pub pulse: BarrierPulseVfxConfig,
}

impl BarrierVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.emissive_brightness, "vfx.barriers.emissive_brightness")?;
        validate_unit_ratio(self.opacity, "vfx.barriers.opacity")?;
        validate_unit_ratio(self.pulse.min_opacity, "vfx.barriers.pulse.min_opacity")?;
        validate_non_negative_finite(self.pulse.frequency_hz, "vfx.barriers.pulse.frequency_hz")?;
        if self.pulse.min_opacity > self.opacity {
            bail!("vfx.barriers.pulse.min_opacity must not exceed vfx.barriers.opacity");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BarrierPulseVfxConfig {
    pub min_opacity: f32,
    pub frequency_hz: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct EraserVfxConfig {
    pub emissive_brightness: f32,
    pub opacity: f32,
}

impl EraserVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.emissive_brightness, "vfx.erasers.emissive_brightness")?;
        validate_unit_ratio(self.opacity, "vfx.erasers.opacity")
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LightBridgeVfxConfig {
    pub emissive_brightness: f32,
    pub opacity: f32,
    pub unpowered_opacity: f32,
    pub fade_secs: f32,
}

impl LightBridgeVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.emissive_brightness, "vfx.light_bridges.emissive_brightness")?;
        validate_unit_ratio(self.opacity, "vfx.light_bridges.opacity")?;
        validate_unit_ratio(self.unpowered_opacity, "vfx.light_bridges.unpowered_opacity")?;
        validate_positive_finite(self.fade_secs, "vfx.light_bridges.fade_secs")?;
        if self.unpowered_opacity > self.opacity {
            bail!("vfx.light_bridges.unpowered_opacity must not exceed vfx.light_bridges.opacity");
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/vfx.rs"]
mod tests;
