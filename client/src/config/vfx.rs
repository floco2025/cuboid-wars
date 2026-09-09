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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn invalid_visual_controls_report_their_config_paths() {
        for (path, invalid) in [
            ("pickups.emissive_brightness", -1.0),
            ("barriers.emissive_brightness", -1.0),
            ("barriers.opacity", 1.1),
            ("barriers.pulse.min_opacity", -0.1),
            ("barriers.pulse.min_opacity", 0.6),
            ("barriers.pulse.frequency_hz", -1.0),
            ("erasers.emissive_brightness", -1.0),
            ("erasers.opacity", -0.1),
            ("erasers.opacity", 1.1),
            ("light_bridges.emissive_brightness", -1.0),
            ("light_bridges.opacity", 1.1),
            ("light_bridges.unpowered_opacity", -0.1),
            ("light_bridges.unpowered_opacity", 0.9),
            ("light_bridges.fade_secs", 0.0),
        ] {
            let mut value = json!({
                "pickups": { "emissive_brightness": 1.0 },
                "barriers": { "emissive_brightness": 2.0, "opacity": 0.5, "pulse": { "min_opacity": 0.1, "frequency_hz": 0.5 } },
                "erasers": { "emissive_brightness": 3.0, "opacity": 0.2 },
                "light_bridges": { "emissive_brightness": 1.0, "opacity": 0.5, "unpowered_opacity": 0.1, "fade_secs": 0.25 }
            });
            let mut field = &mut value;
            for key in path.split('.') {
                field = &mut field[key];
            }
            *field = json!(invalid);
            let config: VfxConfig =
                serde_json::from_value(value).expect("invalid-value VFX fixture failed to deserialize");
            let error = config.validate().expect_err("invalid VFX setting accepted");
            assert!(
                error.to_string().contains(&format!("vfx.{path}")),
                "wrong path in {error}"
            );
        }
    }

    #[test]
    fn emission_and_opacity_can_be_zero() {
        let config: VfxConfig = serde_json::from_value(json!({
            "pickups": { "emissive_brightness": 0.0 },
            "barriers": {
                "emissive_brightness": 0.0,
                "opacity": 0.0,
                "pulse": { "min_opacity": 0.0, "frequency_hz": 0.0 }
            },
            "erasers": { "emissive_brightness": 0.0, "opacity": 0.0 },
            "light_bridges": { "emissive_brightness": 0.0, "opacity": 0.0, "unpowered_opacity": 0.0, "fade_secs": 0.25 }
        }))
        .expect("zero-value VFX config failed to deserialize");
        config.validate().expect("zero emission or opacity rejected");
    }
}
