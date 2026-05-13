use std::{fs, path::Path};

use anyhow::{Context, Result};
use bevy::prelude::Resource;
use serde::Deserialize;

const SUPPORTED_VERSION: u32 = 1;

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

// Three-way map debug-color mode. Cycled at runtime via the C key.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DebugColorMode {
    // Real materials (textures from `assets.json`).
    #[default]
    Off,
    // One color per material name (deterministic hash → HSV). Same material
    // shows the same color across the whole map; lets you visually identify
    // which surfaces share a material.
    ByMaterial,
    // One color per record sent in `MapLayout` (random per batch). Matches
    // exactly what the server emitted — useful for verifying merge behavior.
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

// Per-purpose font sizes. Grouped together so the JSON keeps related knobs
// adjacent, and so callers can resolve a size via `font_sizes.<purpose>`
// without juggling three top-level fields.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FontSizesConfig {
    // HUD overlay text (player list rows, RTT/FPS readouts).
    pub hud: f32,
    // Bottom-right game-message feed lines.
    pub message_feed: f32,
    // Name text inside the 3D floating label above a character. Rendered
    // to a small texture and scaled, so this is typically larger than the
    // HUD size — it has to fill a 256×64 texture before being scaled down.
    pub floating_label: f32,
}

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct RenderSettings {
    pub version: u32,
    pub fov_first_person_degrees: f32,
    pub fov_top_down_degrees: f32,
    pub fov_rearview_degrees: f32,
    pub light_ambient_brightness: f32,
    pub light_directional_brightness: f32,
    pub texture_anisotropy: u16,
    pub rearview_enabled: bool,
    pub opaque_renderer: OpaqueRenderer,
    pub shadows_directional_enabled: bool,
    pub debug_collider_boxes: bool,
    pub texture_mipmaps_enabled: bool,
    pub msaa_samples: u32,
    // Seconds a single message-feed line (kill, key, join, etc.) stays
    // visible before fading out. Player-tunable so spectators can keep
    // entries up longer than active players want.
    pub message_feed_entry_duration_secs: f32,
    pub font_sizes: FontSizesConfig,
}

impl RenderSettings {
    pub fn load_default() -> Result<Self> {
        let settings = Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/client/render.json"
        )))?;
        settings.validate()?;
        Ok(settings)
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.version == SUPPORTED_VERSION,
            "unsupported render config version {} (expected {})",
            self.version,
            SUPPORTED_VERSION
        );
        validate_fov(self.fov_first_person_degrees, "fov_first_person_degrees")?;
        validate_fov(self.fov_top_down_degrees, "fov_top_down_degrees")?;
        validate_fov(self.fov_rearview_degrees, "fov_rearview_degrees")?;
        validate_non_negative_finite(self.light_ambient_brightness, "light_ambient_brightness")?;
        validate_non_negative_finite(self.light_directional_brightness, "light_directional_brightness")?;
        anyhow::ensure!(
            matches!(self.msaa_samples, 1 | 2 | 4 | 8),
            "msaa_samples must be one of 1, 2, 4, or 8"
        );
        validate_non_negative_finite(
            self.message_feed_entry_duration_secs,
            "message_feed_entry_duration_secs",
        )?;
        validate_positive_finite(self.font_sizes.hud, "font_sizes.hud")?;
        validate_positive_finite(self.font_sizes.message_feed, "font_sizes.message_feed")?;
        validate_positive_finite(self.font_sizes.floating_label, "font_sizes.floating_label")?;
        Ok(())
    }
}

fn validate_positive_finite(value: f32, name: &str) -> Result<()> {
    anyhow::ensure!(value.is_finite() && value > 0.0, "{name} must be positive and finite");
    Ok(())
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
