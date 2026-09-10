use std::{fs, path::Path};

use anyhow::{Result, bail};
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

use crate::constants::{
    CAMERA_FOV_DEGREES_DEFAULT, CAMERA_REARVIEW_MIRROR_DEFAULT, CAMERA_SHAKE_SCALE_DEFAULT,
    HUD_SHOW_DIAGNOSTICS_DEFAULT, INPUT_INVERT_Y_DEFAULT, INPUT_MOUSE_SENSITIVITY_DEFAULT,
    INPUT_ZOOM_SENSITIVITY_DEFAULT, RENDERING_FULLSCREEN_RESOLUTION_DEFAULT, RENDERING_MSAA_SAMPLES_DEFAULT,
    RENDERING_PORTAL_VIEW_BUDGET_DEFAULT, RENDERING_VSYNC_DEFAULT,
};

use super::{
    audio::AudioConfig, camera::CameraConfig, hud::HudConfig, interpolation::InterpolationConfig,
    rendering::RenderingConfig, vfx::VfxConfig,
};

// JSON tuning and runtime preferences; local settings override only preferences.
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct ClientSettings {
    pub rendering: RenderingConfig,
    pub interpolation: InterpolationConfig,
    pub camera: CameraConfig,
    #[serde(skip)]
    pub preferences: UserPreferences,
    pub hud: HudConfig,
    pub grass: GrassConfig,
    pub vfx: VfxConfig,
    pub audio: AudioConfig,
    pub weather: WeatherConfig,
    pub lighting: LightingConfig,
}

// One entry per server lighting level (`/light bright|dim|dark`).
// Decoupled from weather — rain does not dim the world.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LightingConfig {
    pub bright: SunLighting,
    pub dim: MoonLighting,
    pub dark: MoonLighting,
}

// Bright is daylight: the disc is the sun, always full. All raw values;
// disc tints are `CELESTIAL_DISC_SUN_COLOR`/`CELESTIAL_DISC_MOON_COLOR` in `constants.rs`.
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

impl WeatherConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.rain_drops_per_second, "weather.rain_drops_per_second")?;
        validate_positive_finite(self.rain_drop_size, "weather.rain_drop_size")?;
        validate_positive_finite(self.rain_spawn_radius, "weather.rain_spawn_radius")?;
        validate_unit_ratio(self.spawn_lead_fraction, "weather.spawn_lead_fraction")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UserPreferences {
    pub fullscreen_resolution: u32,
    pub vsync: bool,
    pub msaa_samples: u32,
    pub portal_view_budget: u8,
    pub mouse_sensitivity: f32,
    pub zoom_sensitivity: f32,
    pub invert_y: bool,
    pub fov_degrees: f32,
    pub shake_scale: f32,
    pub show_diagnostics: bool,
    pub rearview_mirror: bool,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            fullscreen_resolution: RENDERING_FULLSCREEN_RESOLUTION_DEFAULT,
            vsync: RENDERING_VSYNC_DEFAULT,
            msaa_samples: RENDERING_MSAA_SAMPLES_DEFAULT,
            portal_view_budget: RENDERING_PORTAL_VIEW_BUDGET_DEFAULT,
            mouse_sensitivity: INPUT_MOUSE_SENSITIVITY_DEFAULT,
            zoom_sensitivity: INPUT_ZOOM_SENSITIVITY_DEFAULT,
            invert_y: INPUT_INVERT_Y_DEFAULT,
            fov_degrees: CAMERA_FOV_DEGREES_DEFAULT,
            shake_scale: CAMERA_SHAKE_SCALE_DEFAULT,
            show_diagnostics: HUD_SHOW_DIAGNOSTICS_DEFAULT,
            rearview_mirror: CAMERA_REARVIEW_MIRROR_DEFAULT,
        }
    }
}

// Performance/feel knobs for the decorative grass. Pure-appearance numbers
// (blade shape, colors) are module constants in `map/grass/mesh.rs`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct GrassConfig {
    pub enabled: bool,
    pub tufts_per_m2: f32,
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

    pub(crate) fn validate(&self) -> Result<()> {
        self.rendering.validate()?;
        self.interpolation.validate()?;
        self.camera.validate()?;
        self.preferences.validate()?;
        self.hud.validate()?;
        self.grass.validate()?;
        self.vfx.validate()?;
        self.audio.validate()?;
        self.weather.validate()?;
        self.lighting.validate()?;
        Ok(())
    }
}

impl UserPreferences {
    pub(super) fn validate(&self) -> Result<()> {
        validate_positive_finite(self.mouse_sensitivity, "mouse_sensitivity")?;
        validate_positive_finite(self.zoom_sensitivity, "zoom_sensitivity")?;
        validate_fov(self.fov_degrees, "fov_degrees")?;
        validate_non_negative_finite(self.shake_scale, "shake_scale")?;
        if !matches!(self.msaa_samples, 1 | 2 | 4 | 8) {
            bail!("msaa_samples must be one of 1, 2, 4, or 8");
        }
        if self.fullscreen_resolution == 0 {
            bail!("fullscreen_resolution must be > 0");
        }
        if self.portal_view_budget > 8 {
            bail!("portal_view_budget must be <= 8");
        }
        Ok(())
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

    #[test]
    fn preferences_reject_zero_fullscreen_resolution() {
        let mut settings = ClientSettings::load_default().expect("shipped client config should load");
        settings.preferences.fullscreen_resolution = 0;
        let error = settings
            .preferences
            .validate()
            .expect_err("zero render resolution should fail");
        assert!(error.to_string().contains("fullscreen_resolution"));
    }

    #[test]
    fn preferences_reject_portal_budget_above_settings_maximum() {
        let mut settings = ClientSettings::load_default().expect("shipped client config should load");
        settings.preferences.portal_view_budget = 9;
        let error = settings
            .preferences
            .validate()
            .expect_err("oversized portal view budget should fail");
        assert!(error.to_string().contains("portal_view_budget"));
    }
    #[test]
    fn json_cannot_override_runtime_preference_defaults() {
        let mut json: serde_json::Value =
            serde_json::from_str(include_str!("../../../config/client/client.json")).expect("client JSON is invalid");
        json["preferences"] = serde_json::json!({"fov_degrees": 10.0, "zoom_sensitivity": 100.0});
        let settings: ClientSettings = serde_json::from_value(json).expect("client settings are invalid");
        settings.validate().expect("default preferences are invalid");
        assert_eq!(settings.preferences.fov_degrees, CAMERA_FOV_DEGREES_DEFAULT);
        assert_eq!(settings.preferences.zoom_sensitivity, INPUT_ZOOM_SENSITIVITY_DEFAULT);
    }
}
