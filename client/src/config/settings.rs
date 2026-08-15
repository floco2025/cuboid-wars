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
    pub weather: WeatherConfig,
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
    // Constant emissive brightness multiplier on the kind color — set once
    // on the material, never pulsed. Translucency still attenuates what the
    // surface contributes, so useful values are well above the bloom
    // threshold. 0.0 = no glow.
    #[serde(default = "default_barrier_emissive")]
    pub emissive: f32,
}

const fn default_barrier_emissive() -> f32 {
    30.0
}

// Rain presentation knobs. The server owns when it rains (per-map schedule
// in the server gameplay config); these only shape how a given intensity
// looks and sounds on this client.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct WeatherConfig {
    // Skybox + sun-disc brightness factor at full rain (1.0 = no darkening).
    pub sky_dim: f32,
    // Directional + ambient light factor at full rain.
    pub light_dim: f32,
    // Scene saturation factor at full rain (camera color grading) — heavy
    // rain washes the world gray. 1.0 = no change.
    pub saturation: f32,
    pub drops_per_second: f32,
    // Radius of the drop-spawn disc around the camera.
    pub spawn_radius: f32,
    // How far the disc leads the camera along its horizontal facing, as a
    // fraction of `spawn_radius` — 1/3 puts two thirds of the rain ahead of
    // a running player; 0.0 centers it.
    pub spawn_lead_fraction: f32,
    // How far above the camera drops spawn (m) — the height of the rain
    // volume you see when looking up. Taller rain means longer flight times,
    // so live drops ≈ drops_per_second × (spawn_height + 14) / fall_speed;
    // keep that under the `vfx.max_transient_particles` budget.
    pub spawn_height: f32,
    pub fall_speed: f32,
    pub drop_size: f32,
    // Size of the droplets that bounce up where a drop lands.
    pub splash_size: f32,
    // How far those droplets scatter horizontally (m) and how high they
    // bounce (m); velocities and airtime are derived from these.
    pub splash_radius: f32,
    pub splash_height: f32,
    // Rain-loop gain at full intensity.
    pub rain_volume: f32,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            sky_dim: 0.07,
            light_dim: 0.4,
            saturation: 0.5,
            drops_per_second: 300.0,
            spawn_radius: 12.0,
            spawn_lead_fraction: 0.33,
            spawn_height: 12.0,
            fall_speed: 14.0,
            drop_size: 0.012,
            splash_size: 0.012,
            splash_radius: 0.15,
            splash_height: 0.08,
            rain_volume: 1.0,
        }
    }
}

impl WeatherConfig {
    fn validate(&self) -> Result<()> {
        validate_unit_ratio(self.sky_dim, "weather.sky_dim")?;
        validate_unit_ratio(self.light_dim, "weather.light_dim")?;
        validate_unit_ratio(self.saturation, "weather.saturation")?;
        validate_non_negative_finite(self.drops_per_second, "weather.drops_per_second")?;
        validate_positive_finite(self.spawn_radius, "weather.spawn_radius")?;
        validate_unit_ratio(self.spawn_lead_fraction, "weather.spawn_lead_fraction")?;
        validate_positive_finite(self.spawn_height, "weather.spawn_height")?;
        validate_positive_finite(self.fall_speed, "weather.fall_speed")?;
        validate_positive_finite(self.drop_size, "weather.drop_size")?;
        validate_positive_finite(self.splash_size, "weather.splash_size")?;
        validate_non_negative_finite(self.splash_radius, "weather.splash_radius")?;
        // Strictly positive: the droplet airtime is derived from the height,
        // and zero height would divide by a zero airtime.
        validate_positive_finite(self.splash_height, "weather.splash_height")?;
        validate_non_negative_finite(self.rain_volume, "weather.rain_volume")
    }
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
        self.weather.validate()?;
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
        validate_non_negative_finite(self.emissive, "barriers.emissive")?;
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

    #[test]
    fn weather_config_default_validates() {
        WeatherConfig::default()
            .validate()
            .expect("default weather config should validate");
    }

    #[test]
    fn weather_config_rejects_dim_above_one() {
        let config = WeatherConfig {
            sky_dim: 1.5,
            ..WeatherConfig::default()
        };
        let error = config.validate().expect_err("out-of-range sky_dim should fail");
        assert!(error.to_string().contains("sky_dim"));
    }
}
