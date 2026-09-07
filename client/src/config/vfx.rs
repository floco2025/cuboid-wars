use anyhow::{Result, bail};
use serde::Deserialize;

use super::settings::{validate_non_negative_finite, validate_positive_finite, validate_unit_ratio};

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default)]
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
#[serde(default)]
pub struct PickupVfxConfig {
    pub emissive_brightness: f32,
}

impl Default for PickupVfxConfig {
    fn default() -> Self {
        Self {
            emissive_brightness: 1.0,
        }
    }
}

impl PickupVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.emissive_brightness, "vfx.pickups.emissive_brightness")
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct BarrierVfxConfig {
    pub emissive_brightness: f32,
    pub opacity: f32,
    pub pulse: BarrierPulseVfxConfig,
}

impl Default for BarrierVfxConfig {
    fn default() -> Self {
        Self {
            emissive_brightness: 2000.0,
            opacity: 0.015,
            pulse: BarrierPulseVfxConfig::default(),
        }
    }
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
#[serde(default)]
pub struct BarrierPulseVfxConfig {
    pub min_opacity: f32,
    pub frequency_hz: f32,
}

impl Default for BarrierPulseVfxConfig {
    fn default() -> Self {
        Self {
            min_opacity: 0.007,
            frequency_hz: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct EraserVfxConfig {
    pub emissive_brightness: f32,
    pub opacity: f32,
}

impl Default for EraserVfxConfig {
    fn default() -> Self {
        Self {
            emissive_brightness: 2000.0,
            opacity: 0.015,
        }
    }
}

impl EraserVfxConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.emissive_brightness, "vfx.erasers.emissive_brightness")?;
        validate_unit_ratio(self.opacity, "vfx.erasers.opacity")
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct LightBridgeVfxConfig {
    pub emissive_brightness: f32,
    pub opacity: f32,
    pub unpowered_opacity: f32,
    pub fade_secs: f32,
}

impl Default for LightBridgeVfxConfig {
    fn default() -> Self {
        Self {
            emissive_brightness: 6.0,
            opacity: 0.8,
            unpowered_opacity: 0.15,
            fade_secs: 0.25,
        }
    }
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
    fn partial_settings_keep_defaults_for_other_visual_controls() {
        let config: VfxConfig = serde_json::from_value(json!({
            "barriers": { "opacity": 0.4 },
            "erasers": { "opacity": 0.2, "emissive_brightness": 9.0 },
            "light_bridges": { "fade_secs": 0.5 }
        }))
        .expect("partial VFX config failed to deserialize");
        config.validate().expect("partial VFX config invalid");
        assert_eq!(config.pickups.emissive_brightness, 1.0);
        assert_eq!(config.barriers.opacity, 0.4);
        assert_eq!(config.barriers.emissive_brightness, 2000.0);
        assert_eq!(config.barriers.pulse.min_opacity, 0.007);
        assert_eq!(config.erasers.opacity, 0.2);
        assert_eq!(config.erasers.emissive_brightness, 9.0);
        assert_eq!(config.light_bridges.emissive_brightness, 6.0);
        assert_eq!(config.light_bridges.fade_secs, 0.5);
    }

    #[test]
    fn invalid_visual_controls_report_their_config_paths() {
        for (path, invalid) in [
            ("pickups.emissive_brightness", -1.0),
            ("barriers.emissive_brightness", -1.0),
            ("barriers.opacity", 1.1),
            ("barriers.pulse.min_opacity", -0.1),
            ("barriers.pulse.min_opacity", 0.02),
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
            let mut value = json!({});
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
            "light_bridges": { "emissive_brightness": 0.0, "opacity": 0.0, "unpowered_opacity": 0.0 }
        }))
        .expect("zero-value VFX config failed to deserialize");
        config.validate().expect("zero emission or opacity rejected");
    }
}
