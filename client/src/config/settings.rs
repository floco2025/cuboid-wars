use std::{fs, path::Path};

use anyhow::{Result, bail};
use bevy::prelude::Resource;
use serde::Deserialize;

use super::{
    audio::AudioConfig,
    camera::CameraConfig,
    hud::HudConfig,
    rendering::{LightingConfig, RenderingConfig},
    vfx::VfxConfig,
};

const SUPPORTED_VERSION: u32 = 1;

// Three-way map debug-color mode. Cycled at runtime via the C key.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DebugColorMode {
    // Real materials (textures from `assets.json`).
    #[default]
    Off,
    // One color per material name (deterministic hash → HSV).
    ByMaterial,
    // One color per record sent in `MapLayout` (random per batch).
    BySegment,
}

impl DebugColorMode {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::ByMaterial,
            Self::ByMaterial => Self::BySegment,
            Self::BySegment => Self::Off,
        }
    }
}

// Top-level client config. Loaded from `config/client/client.json` once at
// startup; fields are immutable after that (no hot-reload).
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct ClientSettings {
    pub version: u32,
    pub rendering: RenderingConfig,
    pub lighting: LightingConfig,
    pub camera: CameraConfig,
    pub input: InputConfig,
    pub hud: HudConfig,
    pub barriers: BarriersConfig,
    #[serde(default)]
    pub grass: GrassConfig,
    #[serde(default)]
    pub vfx: VfxConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    pub debug: DebugConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct InputConfig {
    // Mouse look sensitivity in radians per pixel. Larger = faster turn
    // per inch of mouse movement.
    pub mouse_sensitivity: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BarriersConfig {
    // Pulse alpha swings between `alpha_min` and `alpha_max` at the
    // configured rate. Below ~0.1 the barrier almost disappears (good
    // off-phase look); above ~0.7 it reads as solid.
    pub alpha_min: f32,
    pub alpha_max: f32,
    pub pulse_hz: f32,
}

// Performance/feel knobs for the decorative grass. Pure-appearance numbers
// (blade shape, colors) are module constants in `map/spawn/grass.rs`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct GrassConfig {
    pub enabled: bool,
    pub tufts_per_m2: f32,
    // Horizontal sway amplitude at the blade tip, in meters.
    pub wind_strength: f32,
    // Sway oscillation speed, in radians per second.
    pub wind_speed: f32,
    pub wind_direction_degrees: f32,
}

impl Default for GrassConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tufts_per_m2: 12.0,
            wind_strength: 0.05,
            wind_speed: 1.5,
            wind_direction_degrees: 30.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct DebugConfig {
    pub collider_boxes: bool,
}

impl ClientSettings {
    pub fn load_default() -> Result<Self> {
        let settings = Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/client/client.json"
        )))?;
        settings.validate()?;
        Ok(settings)
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        if self.version != SUPPORTED_VERSION {
            bail!(
                "unsupported client config version {} (expected {})",
                self.version,
                SUPPORTED_VERSION
            );
        }
        self.rendering.validate()?;
        self.lighting.validate()?;
        self.camera.validate()?;
        self.input.validate()?;
        self.hud.validate()?;
        self.barriers.validate()?;
        self.grass.validate()?;
        self.vfx.validate()?;
        self.audio.validate()?;
        Ok(())
    }
}

impl InputConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.mouse_sensitivity, "input.mouse_sensitivity")
    }
}

impl BarriersConfig {
    fn validate(&self) -> Result<()> {
        validate_unit_ratio(self.alpha_min, "barriers.alpha_min")?;
        validate_unit_ratio(self.alpha_max, "barriers.alpha_max")?;
        if self.alpha_min > self.alpha_max {
            bail!("barriers.alpha_min must be <= barriers.alpha_max");
        }
        validate_positive_finite(self.pulse_hz, "barriers.pulse_hz")?;
        Ok(())
    }
}

impl GrassConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.tufts_per_m2, "grass.tufts_per_m2")?;
        validate_non_negative_finite(self.wind_strength, "grass.wind_strength")?;
        validate_non_negative_finite(self.wind_speed, "grass.wind_speed")?;
        if !self.wind_direction_degrees.is_finite() {
            bail!("grass.wind_direction_degrees must be finite");
        }
        Ok(())
    }
}

pub(super) fn validate_positive_finite(value: f32, name: &str) -> Result<()> {
    if !(value.is_finite() && value > 0.0) {
        bail!("{name} must be positive and finite");
    }
    Ok(())
}

pub(super) fn validate_non_negative_finite(value: f32, name: &str) -> Result<()> {
    if !(value.is_finite() && value >= 0.0) {
        bail!("{name} must be finite and non-negative");
    }
    Ok(())
}

pub(super) fn validate_fov(fov_degrees: f32, name: &str) -> Result<()> {
    if !(1.0..179.0).contains(&fov_degrees) {
        bail!("{name} must be greater than 1 and less than 179");
    }
    Ok(())
}

pub(super) fn validate_unit_ratio(value: f32, name: &str) -> Result<()> {
    if !(value.is_finite() && (0.0..=1.0).contains(&value)) {
        bail!("{name} must be in [0.0, 1.0]");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_client_config_loads_and_validates() {
        ClientSettings::load_default().expect("shipped client config should load and validate");
    }
}
