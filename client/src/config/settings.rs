use std::{fs, path::Path};

use anyhow::{Result, bail};
use bevy::prelude::{Color, Resource};
use common::protocol::HexColor;
use serde::{Deserialize, Serialize};

use crate::constants::{
    AUDIO_ACTOR_MOVEMENT_VOLUME_DB_DEFAULT, AUDIO_FOOTSTEP_VOLUME_DB_DEFAULT, AUDIO_VOLUME_DB_MAX, AUDIO_VOLUME_DB_MIN,
    CAMERA_FOV_DEGREES_DEFAULT, CAMERA_SHAKE_SCALE_DEFAULT, HUD_SHOW_DIAGNOSTICS_DEFAULT, INPUT_INVERT_Y_DEFAULT,
    INPUT_MOUSE_SENSITIVITY_DEFAULT, INPUT_ZOOM_SENSITIVITY_DEFAULT, RENDERING_FULLSCREEN_RESOLUTION_DEFAULT,
    RENDERING_MSAA_SAMPLES_DEFAULT, RENDERING_PORTAL_VIEW_BUDGET_DEFAULT, RENDERING_VSYNC_DEFAULT,
    SKY_MAX_BODY_SIZE_SCALE,
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
    pub sky: SkyConfig,
    pub lighting: LightingConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightingConfig {
    pub day_ambient_brightness: f32,
    pub twilight_ambient_brightness: f32,
    pub night_ambient_brightness: f32,
    pub max_sun_illuminance: f32,
    pub max_full_moon_illuminance: f32,
    pub shadow_step_degrees: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyConfig {
    pub day_brightness: f32,
    pub twilight_brightness: f32,
    pub night_brightness: f32,
    pub sun: BodySkyConfig,
    pub moon: BodySkyConfig,
    pub stars: StarSkyConfig,
    pub clouds: CloudSkyConfig,
}

// The sun's or the moon's apparent size, as a multiple of its real mean
// radius, and its disc luminance.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodySkyConfig {
    pub size_scale: f32,
    pub luminance: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StarSkyConfig {
    pub density: f32,
    pub luminance: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudSkyConfig {
    pub clear_coverage: f32,
    pub overcast_coverage: f32,
    // World-aligned angular drift per real second. The procedural growth
    // cycle derives from the same rate.
    pub movement_speed_degrees_per_second: f32,
}

impl LightingConfig {
    fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("day_ambient_brightness", self.day_ambient_brightness),
            ("twilight_ambient_brightness", self.twilight_ambient_brightness),
            ("night_ambient_brightness", self.night_ambient_brightness),
            ("max_sun_illuminance", self.max_sun_illuminance),
            ("max_full_moon_illuminance", self.max_full_moon_illuminance),
            ("shadow_step_degrees", self.shadow_step_degrees),
        ] {
            validate_non_negative_finite(value, &format!("lighting.{name}"))?;
        }
        if self.shadow_step_degrees >= 180.0 {
            bail!("lighting.shadow_step_degrees must be less than 180");
        }
        Ok(())
    }
}

impl SkyConfig {
    fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("day_brightness", self.day_brightness),
            ("twilight_brightness", self.twilight_brightness),
            ("night_brightness", self.night_brightness),
            ("sun.luminance", self.sun.luminance),
            ("moon.luminance", self.moon.luminance),
            ("stars.luminance", self.stars.luminance),
        ] {
            validate_non_negative_finite(value, &format!("sky.{name}"))?;
        }
        for (name, value) in [
            ("sun.size_scale", self.sun.size_scale),
            ("moon.size_scale", self.moon.size_scale),
        ] {
            validate_positive_finite(value, &format!("sky.{name}"))?;
            if value > SKY_MAX_BODY_SIZE_SCALE {
                bail!("sky.{name} must be <= {SKY_MAX_BODY_SIZE_SCALE}");
            }
        }
        for (name, value) in [
            ("stars.density", self.stars.density),
            ("clouds.clear_coverage", self.clouds.clear_coverage),
            ("clouds.overcast_coverage", self.clouds.overcast_coverage),
        ] {
            validate_unit_ratio(value, &format!("sky.{name}"))?;
        }
        validate_non_negative_finite(
            self.clouds.movement_speed_degrees_per_second,
            "sky.clouds.movement_speed_degrees_per_second",
        )?;
        if self.clouds.clear_coverage > self.clouds.overcast_coverage {
            bail!("sky.clouds.clear_coverage must be <= sky.clouds.overcast_coverage");
        }
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
    pub footstep_volume_db: f32,
    pub actor_movement_volume_db: f32,
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
            footstep_volume_db: AUDIO_FOOTSTEP_VOLUME_DB_DEFAULT,
            actor_movement_volume_db: AUDIO_ACTOR_MOVEMENT_VOLUME_DB_DEFAULT,
        }
    }
}

// Grass density, shape, LOD, and wind are cohesive art-direction constants.
// The base color remains configurable because it is routinely tuned against
// the terrain textures and scene lighting.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrassConfig {
    pub enabled: bool,
    pub color: HexColor,
}

impl GrassConfig {
    pub fn base_color(self) -> Color {
        let [red, green, blue] = self.color.0;
        Color::srgb_u8(red, green, blue)
    }
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
        self.vfx.validate()?;
        self.audio.validate()?;
        self.weather.validate()?;
        self.sky.validate()?;
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
        for (name, value) in [
            ("footstep_volume_db", self.footstep_volume_db),
            ("actor_movement_volume_db", self.actor_movement_volume_db),
        ] {
            if !(AUDIO_VOLUME_DB_MIN..=AUDIO_VOLUME_DB_MAX).contains(&value) {
                bail!("{name} must be in [{AUDIO_VOLUME_DB_MIN}, {AUDIO_VOLUME_DB_MAX}]");
            }
        }
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
#[path = "tests/settings.rs"]
mod tests;
