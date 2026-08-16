use anyhow::Result;
use serde::Deserialize;

use super::settings::{validate_fov, validate_non_negative_finite, validate_positive_finite, validate_unit_ratio};

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CameraConfig {
    pub fov_first_person_degrees: f32,
    pub fov_top_down_degrees: f32,
    // Padding factor applied around the visible map when fitting the
    // top-down camera. 1.0 = exact fit, >1.0 = room around edges.
    pub topdown_margin: f32,
    pub topdown_tilt_degrees: f32,
    pub rearview: RearviewConfig,
    #[serde(default)]
    pub shake: CameraShakeConfig,
}

// Directional camera shake on the local player, fired for projectile hits
// and laser burn (along the incoming direction, with a small vertical
// companion) and hard landings (vertical only). NOT for blasts — they
// already have knockback, so shake on top reads as double feedback.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct CameraShakeConfig {
    pub projectile: ShakeSourceConfig,
    pub laser: ShakeSourceConfig,
    pub fall: ShakeSourceConfig,
}

// One damage source's shake: `intensity` is the amplitude; `vertical_ratio`
// is a RATIO of that intensity (the horizontal hit direction is unit
// length, so vertical strength = intensity × vertical_ratio).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ShakeSourceConfig {
    pub intensity: f32,
    pub vertical_ratio: f32,
    pub duration_secs: f32,
}

impl Default for ShakeSourceConfig {
    fn default() -> Self {
        Self {
            intensity: 0.2,
            vertical_ratio: 0.2,
            duration_secs: 0.3,
        }
    }
}

impl Default for CameraShakeConfig {
    fn default() -> Self {
        Self {
            projectile: ShakeSourceConfig::default(),
            laser: ShakeSourceConfig::default(),
            fall: ShakeSourceConfig {
                vertical_ratio: 0.5,
                ..ShakeSourceConfig::default()
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RearviewConfig {
    pub enabled: bool,
    pub fov_degrees: f32,
    // Width / height of the rearview viewport as a fraction of the window
    // dimensions. The inset from the window edge is a fixed `HUD_EDGE_MARGIN_PX`
    // shared with the HUD panels, not a ratio.
    pub width_ratio: f32,
    pub height_ratio: f32,
}

impl CameraConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_fov(self.fov_first_person_degrees, "camera.fov_first_person_degrees")?;
        validate_fov(self.fov_top_down_degrees, "camera.fov_top_down_degrees")?;
        validate_positive_finite(self.topdown_margin, "camera.topdown_margin")?;
        validate_positive_finite(self.topdown_tilt_degrees, "camera.topdown_tilt_degrees")?;
        self.rearview.validate()?;
        self.shake.validate()?;
        Ok(())
    }
}

impl CameraShakeConfig {
    fn validate(&self) -> Result<()> {
        self.projectile.validate("camera.shake.projectile")?;
        self.laser.validate("camera.shake.laser")?;
        self.fall.validate("camera.shake.fall")
    }
}

impl ShakeSourceConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.intensity, &format!("{path}.intensity"))?;
        validate_non_negative_finite(self.vertical_ratio, &format!("{path}.vertical_ratio"))?;
        validate_positive_finite(self.duration_secs, &format!("{path}.duration_secs"))
    }
}

impl RearviewConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_fov(self.fov_degrees, "camera.rearview.fov_degrees")?;
        validate_unit_ratio(self.width_ratio, "camera.rearview.width_ratio")?;
        validate_unit_ratio(self.height_ratio, "camera.rearview.height_ratio")?;
        Ok(())
    }
}
