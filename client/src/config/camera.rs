use anyhow::{Result, ensure};
use serde::Deserialize;

use super::settings::{validate_fov, validate_non_negative_finite, validate_positive_finite, validate_unit_ratio};

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CameraConfig {
    pub follow: FollowCameraConfig,
    pub top_down: TopDownConfig,
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
    // Width / height of the rearview viewport as a fraction of the window
    // dimensions. The inset from the window edge is a fixed `HUD_EDGE_MARGIN_PX`
    // shared with the HUD panels, not a ratio.
    pub width_ratio: f32,
    pub height_ratio: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct TopDownConfig {
    pub fov_degrees: f32,
    pub margin: f32,
    pub tilt_degrees: f32,
}

impl CameraConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_fov(self.top_down.fov_degrees, "camera.top_down.fov_degrees")?;
        validate_positive_finite(self.top_down.margin, "camera.top_down.margin")?;
        validate_positive_finite(self.top_down.tilt_degrees, "camera.top_down.tilt_degrees")?;
        ensure!(
            self.top_down.tilt_degrees < 90.0,
            "camera.top_down.tilt_degrees must be < 90"
        );
        self.follow.validate()?;
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
        validate_unit_ratio(self.width_ratio, "camera.rearview.width_ratio")?;
        validate_unit_ratio(self.height_ratio, "camera.rearview.height_ratio")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FollowCameraConfig {
    pub max_distance: f32,
    pub first_person_distance: f32,
    pub pivot_height: f32,
    pub shoulder_offset: f32,
    pub collision_radius: f32,
    pub obstruction_return_rate: f32,
}

impl FollowCameraConfig {
    fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("max_distance", self.max_distance),
            ("first_person_distance", self.first_person_distance),
            ("pivot_height", self.pivot_height),
            ("collision_radius", self.collision_radius),
            ("obstruction_return_rate", self.obstruction_return_rate),
        ] {
            validate_positive_finite(value, &format!("camera.follow.{name}"))?;
        }
        validate_non_negative_finite(self.shoulder_offset, "camera.follow.shoulder_offset")?;
        ensure!(
            self.first_person_distance < self.max_distance,
            "camera.follow.first_person_distance must be < max_distance"
        );
        Ok(())
    }
}
