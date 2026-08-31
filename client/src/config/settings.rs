use std::{fs, path::Path};

use anyhow::{Result, bail};
use bevy::prelude::Resource;
use serde::Deserialize;

use super::{audio::AudioConfig, camera::CameraConfig, hud::HudConfig, rendering::RenderingConfig, vfx::VfxConfig};

// Top-level client config. Loaded from `config/client/client.json` once at
// startup; fields are immutable after that (no hot-reload).
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct ClientSettings {
    pub rendering: RenderingConfig,
    pub camera: CameraConfig,
    pub input: InputConfig,
    pub hud: HudConfig,
    #[serde(default)]
    pub grass: GrassConfig,
    #[serde(default)]
    pub vfx: VfxConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub weather: WeatherConfig,
    #[serde(default)]
    pub lighting: LightingConfig,
    pub debug: DebugConfig,
}

// One entry per server lighting level (`/light bright|dim|dark`).
// Decoupled from weather — rain does not dim the world.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct LightingConfig {
    pub bright: SunLighting,
    pub dim: MoonLighting,
    pub dark: MoonLighting,
}

// Bright is daylight: the disc is the sun, always full. All raw values;
// disc tints are `SUN_DISC_COLOR`/`MOON_DISC_COLOR` in `constants.rs`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct SunLighting {
    // `Skybox::brightness` (same scale as the per-skybox `brightness` in
    // `assets.json`; the shipped sky's full value is 1000).
    pub sky_brightness: f32,
    // `DirectionalLight::illuminance` (lux) and `AmbientLight::brightness`.
    pub sun_illuminance: f32,
    pub ambient_brightness: f32,
    // Emissive luminance of the visible sun disc (scene-linear; bloom halos
    // anything past the bloom threshold).
    pub sun_disc_luminance: f32,
    // Post-tonemap saturation; 1.0 = unchanged.
    pub saturation: f32,
}

// Dim and dark are moonlight: the directional light is the moon, and the
// disc shows a phase.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MoonLighting {
    pub sky_brightness: f32,
    // `DirectionalLight::illuminance` (lux) and `AmbientLight::brightness`.
    pub moon_illuminance: f32,
    pub ambient_brightness: f32,
    // Emissive luminance of the visible moon disc (scene-linear; 0 = off).
    pub moon_disc_luminance: f32,
    // Lit fraction in percent: 100 = full moon, 50 = half, 35 = crescent.
    pub moon_phase_percent: f32,
    pub saturation: f32,
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            bright: SunLighting {
                sky_brightness: 1000.0,
                sun_illuminance: 8000.0,
                ambient_brightness: 70.0,
                sun_disc_luminance: 100.0,
                saturation: 1.0,
            },
            dim: MoonLighting {
                sky_brightness: 64.0,
                moon_illuminance: 200.0,
                ambient_brightness: 30.0,
                moon_disc_luminance: 5.0,
                moon_phase_percent: 60.0,
                saturation: 0.5,
            },
            dark: MoonLighting {
                sky_brightness: 12.0,
                moon_illuminance: 50.0,
                ambient_brightness: 10.0,
                moon_disc_luminance: 0.1,
                moon_phase_percent: 35.0,
                saturation: 0.3,
            },
        }
    }
}

impl LightingConfig {
    fn validate(&self) -> Result<()> {
        self.bright.validate("lighting.bright")?;
        self.dim.validate("lighting.dim")?;
        self.dark.validate("lighting.dark")?;
        Ok(())
    }
}

impl SunLighting {
    fn validate(&self, name: &str) -> Result<()> {
        validate_non_negative_finite(self.sky_brightness, &format!("{name}.sky_brightness"))?;
        validate_non_negative_finite(self.sun_illuminance, &format!("{name}.sun_illuminance"))?;
        validate_non_negative_finite(self.ambient_brightness, &format!("{name}.ambient_brightness"))?;
        validate_non_negative_finite(self.sun_disc_luminance, &format!("{name}.sun_disc_luminance"))?;
        validate_unit_ratio(self.saturation, &format!("{name}.saturation"))?;
        Ok(())
    }
}

impl MoonLighting {
    fn validate(&self, name: &str) -> Result<()> {
        validate_non_negative_finite(self.sky_brightness, &format!("{name}.sky_brightness"))?;
        validate_non_negative_finite(self.moon_illuminance, &format!("{name}.moon_illuminance"))?;
        validate_non_negative_finite(self.ambient_brightness, &format!("{name}.ambient_brightness"))?;
        validate_non_negative_finite(self.moon_disc_luminance, &format!("{name}.moon_disc_luminance"))?;
        if !(0.0..=100.0).contains(&self.moon_phase_percent) {
            bail!("{name}.moon_phase_percent must be in [0, 100]");
        }
        validate_unit_ratio(self.saturation, &format!("{name}.saturation"))?;
        Ok(())
    }
}

// User-facing rain density/size knobs. Pure-appearance values (colors,
// speeds, splash shape) are constants; structural values live in
// `vfx/rain.rs`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct WeatherConfig {
    // Drop spawn rate at full intensity — the rain density knob.
    pub rain_drops_per_second: f32,
    // Drop cross-section (m).
    pub rain_drop_size: f32,
    // Radius of the drop-spawn disc around the camera (m).
    pub rain_spawn_radius: f32,
    // How far the disc leads the camera along its horizontal facing, as a
    // fraction of the spawn radius — a third puts two thirds of the rain
    // ahead of a running player.
    pub spawn_lead_fraction: f32,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            rain_drops_per_second: 1000.0,
            rain_drop_size: 0.01,
            rain_spawn_radius: 15.0,
            spawn_lead_fraction: 0.35,
        }
    }
}

impl WeatherConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.rain_drops_per_second, "weather.rain_drops_per_second")?;
        validate_positive_finite(self.rain_drop_size, "weather.rain_drop_size")?;
        validate_positive_finite(self.rain_spawn_radius, "weather.rain_spawn_radius")?;
        validate_unit_ratio(self.spawn_lead_fraction, "weather.spawn_lead_fraction")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct InputConfig {
    // Mouse look sensitivity in radians per pixel. Larger = faster turn
    // per inch of mouse movement.
    pub mouse_sensitivity: f32,
    // Flip vertical mouse look (mouse forward = look down).
    #[serde(default)]
    pub invert_y: bool,
}

// Performance/feel knobs for the decorative grass. Pure-appearance numbers
// (blade shape, colors) are module constants in `map/spawn/grass.rs`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct GrassConfig {
    pub enabled: bool,
    pub tufts_per_m2: f32,
}

impl Default for GrassConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tufts_per_m2: 12.0,
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
        self.rendering.validate()?;
        self.camera.validate()?;
        self.input.validate()?;
        self.hud.validate()?;
        self.grass.validate()?;
        self.vfx.validate()?;
        self.audio.validate()?;
        self.weather.validate()?;
        self.lighting.validate()?;
        Ok(())
    }
}

impl InputConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.mouse_sensitivity, "input.mouse_sensitivity")
    }
}

impl GrassConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.tufts_per_m2, "grass.tufts_per_m2")?;
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
