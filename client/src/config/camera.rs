use anyhow::Result;
use serde::Deserialize;

use super::settings::{validate_fov, validate_positive_finite, validate_unit_ratio};

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

// Directional camera shake on the local player: projectile hits shake along
// the incoming shot direction (with a small vertical companion), hard
// landings shake vertically. Both share one duration/intensity envelope.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct CameraShakeConfig {
    pub duration_secs: f32,
    pub intensity: f32,
    pub hit_vertical: f32,
    pub fall_vertical: f32,
    pub blast_vertical: f32,
}

impl Default for CameraShakeConfig {
    fn default() -> Self {
        Self {
            duration_secs: 0.3,
            intensity: 3.0,
            hit_vertical: 0.2,
            fall_vertical: 0.5,
            blast_vertical: 0.35,
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
        Ok(())
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
