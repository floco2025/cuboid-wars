use std::{fs, path::Path};

use anyhow::{Context, Result};
use bevy::prelude::Resource;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpaqueRenderer {
    Auto,
    Forward,
    Deferred,
}

impl OpaqueRenderer {
    #[must_use]
    pub const fn is_deferred(self) -> bool {
        matches!(self, Self::Deferred)
    }
}

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct RenderSettings {
    pub fov_first_person_degrees: f32,
    pub fov_top_down_degrees: f32,
    pub fov_rearview_degrees: f32,
    pub light_ambient_brightness: f32,
    pub light_directional_brightness: f32,
    pub texture_anisotropy: u16,
    pub rearview_enabled: bool,
    pub opaque_renderer: OpaqueRenderer,
    pub shadows_directional_enabled: bool,
    pub shadows_player_enabled: bool,
    pub debug_map_colors: bool,
    pub debug_bounding_boxes: bool,
    pub texture_mipmaps_enabled: bool,
    pub msaa_samples: u32,
}

impl RenderSettings {
    pub fn load_default() -> Result<Self> {
        let settings = Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/render_settings.json"
        )))?;
        settings.validate()?;
        Ok(settings)
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        validate_fov(self.fov_first_person_degrees, "fov_first_person_degrees")?;
        validate_fov(self.fov_top_down_degrees, "fov_top_down_degrees")?;
        validate_fov(self.fov_rearview_degrees, "fov_rearview_degrees")?;
        validate_non_negative_finite(self.light_ambient_brightness, "light_ambient_brightness")?;
        validate_non_negative_finite(self.light_directional_brightness, "light_directional_brightness")?;
        anyhow::ensure!(
            matches!(self.msaa_samples, 1 | 2 | 4 | 8),
            "msaa_samples must be one of 1, 2, 4, or 8"
        );
        Ok(())
    }
}

fn validate_non_negative_finite(value: f32, name: &str) -> Result<()> {
    anyhow::ensure!(
        value.is_finite() && value >= 0.0,
        "{name} must be finite and non-negative"
    );
    Ok(())
}

fn validate_fov(fov_degrees: f32, name: &str) -> Result<()> {
    anyhow::ensure!(
        (1.0..179.0).contains(&fov_degrees),
        "{name} must be greater than 1 and less than 179"
    );
    Ok(())
}
