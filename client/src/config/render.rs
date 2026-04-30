use std::{fs, path::Path};

use anyhow::{Context, Result};
use bevy::prelude::Resource;
use serde::Deserialize;

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct RenderSettings {
    pub fov_first_person_degrees: f32,
    pub fov_top_down_degrees: f32,
    pub fov_rearview_degrees: f32,
    pub texture_anisotropy: u16,
    pub rearview_enabled: bool,
    pub shadows_directional_enabled: bool,
    pub shadows_player_enabled: bool,
    pub map_debug_colors: bool,
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
        anyhow::ensure!(
            matches!(self.msaa_samples, 1 | 2 | 4 | 8),
            "msaa_samples must be one of 1, 2, 4, or 8"
        );
        Ok(())
    }
}

fn validate_fov(fov_degrees: f32, name: &str) -> Result<()> {
    anyhow::ensure!(
        (1.0..179.0).contains(&fov_degrees),
        "{name} must be greater than 1 and less than 179"
    );
    Ok(())
}
