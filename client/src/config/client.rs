use std::{fs, path::Path};

use anyhow::{Result, bail};
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
    pub debug: DebugConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RenderingConfig {
    pub opaque_renderer: OpaqueRenderer,
    pub shadows_directional_enabled: bool,
    pub texture_mipmaps_enabled: bool,
    pub texture_anisotropy: u16,
    pub msaa_samples: u32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LightingConfig {
    pub ambient_brightness: f32,
    pub directional_brightness: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CameraConfig {
    pub fov_first_person_degrees: f32,
    pub fov_top_down_degrees: f32,
    // Padding factor applied around the visible map when fitting the
    // top-down camera. 1.0 = exact fit, >1.0 = room around edges.
    pub topdown_margin: f32,
    pub topdown_tilt_degrees: f32,
    pub rearview: RearviewConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RearviewConfig {
    pub enabled: bool,
    pub fov_degrees: f32,
    // Width / height of the rearview viewport as a fraction of the window
    // dimensions; margin is also a window-fraction (so all three scale
    // together when the window resizes).
    pub width_ratio: f32,
    pub height_ratio: f32,
    pub margin_ratio: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct InputConfig {
    // Mouse look sensitivity in radians per pixel. Larger = faster turn
    // per inch of mouse movement.
    pub mouse_sensitivity: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct HudConfig {
    pub font_sizes: FontSizesConfig,
    pub message_feed: MessageFeedConfig,
    pub floating_labels: FloatingLabelsConfig,
    pub health_bars: HealthBarsConfig,
    pub quest_overlay: QuestOverlayConfig,
}

// Per-purpose font sizes. Each surface has its own preferred size; the
// floating-label one is much larger because it has to fill a small texture
// before being scaled down onto a world-space quad.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FontSizesConfig {
    // Player-list names and the RTT / FPS readouts.
    pub player_list: f32,
    // Score column in the player list. Often larger than `player_list`
    // since it's the headline number.
    pub score: f32,
    // Bottom-right game-message feed lines.
    pub message_feed: f32,
    // Name text inside the 3D floating label above a character.
    pub floating_label: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MessageFeedConfig {
    pub entry_duration_secs: f32,
    pub max_entries: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FloatingLabelsConfig {
    // Beyond this distance from the main camera (meters), character labels
    // stop rendering. Perf knob — combined with `Changed<Health>` gating.
    pub cull_distance: f32,
    pub height_above_character: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct HealthBarsConfig {
    pub floating_player_width: f32,
    pub floating_player_height: f32,
    pub floating_actor_width: f32,
    pub floating_actor_height: f32,
    pub player_list_width: f32,
    pub player_list_height: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct QuestOverlayConfig {
    pub announcement_duration_secs: f32,
    pub achieved_duration_secs: f32,
    pub font_size: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BarriersConfig {
    // Pulse alpha swings between `alpha_min` and `alpha_max` at the
    // configured rate. Below ~0.1 the barrier almost disappears (good
    // off-phase look); above ~0.7 it reads as solid.
    pub alpha_min: f32,
    pub alpha_max: f32,
    pub pulse_hz: f32,
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
        Ok(())
    }
}

impl RenderingConfig {
    fn validate(&self) -> Result<()> {
        if !matches!(self.msaa_samples, 1 | 2 | 4 | 8) {
            bail!("rendering.msaa_samples must be one of 1, 2, 4, or 8");
        }
        Ok(())
    }
}

impl LightingConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.ambient_brightness, "lighting.ambient_brightness")?;
        validate_non_negative_finite(self.directional_brightness, "lighting.directional_brightness")?;
        Ok(())
    }
}

impl CameraConfig {
    fn validate(&self) -> Result<()> {
        validate_fov(self.fov_first_person_degrees, "camera.fov_first_person_degrees")?;
        validate_fov(self.fov_top_down_degrees, "camera.fov_top_down_degrees")?;
        validate_positive_finite(self.topdown_margin, "camera.topdown_margin")?;
        validate_positive_finite(self.topdown_tilt_degrees, "camera.topdown_tilt_degrees")?;
        self.rearview.validate()?;
        Ok(())
    }
}

impl RearviewConfig {
    fn validate(&self) -> Result<()> {
        validate_fov(self.fov_degrees, "camera.rearview.fov_degrees")?;
        validate_unit_ratio(self.width_ratio, "camera.rearview.width_ratio")?;
        validate_unit_ratio(self.height_ratio, "camera.rearview.height_ratio")?;
        validate_unit_ratio(self.margin_ratio, "camera.rearview.margin_ratio")?;
        Ok(())
    }
}

impl InputConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.mouse_sensitivity, "input.mouse_sensitivity")
    }
}

impl HudConfig {
    fn validate(&self) -> Result<()> {
        validate_positive_finite(self.font_sizes.player_list, "hud.font_sizes.player_list")?;
        validate_positive_finite(self.font_sizes.score, "hud.font_sizes.score")?;
        validate_positive_finite(self.font_sizes.message_feed, "hud.font_sizes.message_feed")?;
        validate_positive_finite(self.font_sizes.floating_label, "hud.font_sizes.floating_label")?;
        validate_non_negative_finite(
            self.message_feed.entry_duration_secs,
            "hud.message_feed.entry_duration_secs",
        )?;
        if self.message_feed.max_entries == 0 {
            bail!("hud.message_feed.max_entries must be > 0");
        }
        validate_positive_finite(self.floating_labels.cull_distance, "hud.floating_labels.cull_distance")?;
        validate_positive_finite(
            self.floating_labels.height_above_character,
            "hud.floating_labels.height_above_character",
        )?;
        validate_positive_finite(
            self.health_bars.floating_player_width,
            "hud.health_bars.floating_player_width",
        )?;
        validate_positive_finite(
            self.health_bars.floating_player_height,
            "hud.health_bars.floating_player_height",
        )?;
        validate_positive_finite(
            self.health_bars.floating_actor_width,
            "hud.health_bars.floating_actor_width",
        )?;
        validate_positive_finite(
            self.health_bars.floating_actor_height,
            "hud.health_bars.floating_actor_height",
        )?;
        validate_positive_finite(self.health_bars.player_list_width, "hud.health_bars.player_list_width")?;
        validate_positive_finite(
            self.health_bars.player_list_height,
            "hud.health_bars.player_list_height",
        )?;
        validate_positive_finite(
            self.quest_overlay.announcement_duration_secs,
            "hud.quest_overlay.announcement_duration_secs",
        )?;
        validate_positive_finite(
            self.quest_overlay.achieved_duration_secs,
            "hud.quest_overlay.achieved_duration_secs",
        )?;
        validate_positive_finite(self.quest_overlay.font_size, "hud.quest_overlay.font_size")?;
        Ok(())
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
        Ok(())
    }
}

fn validate_positive_finite(value: f32, name: &str) -> Result<()> {
    if !(value.is_finite() && value > 0.0) {
        bail!("{name} must be positive and finite");
    }
    Ok(())
}

fn validate_non_negative_finite(value: f32, name: &str) -> Result<()> {
    if !(value.is_finite() && value >= 0.0) {
        bail!("{name} must be finite and non-negative");
    }
    Ok(())
}

fn validate_fov(fov_degrees: f32, name: &str) -> Result<()> {
    if !(1.0..179.0).contains(&fov_degrees) {
        bail!("{name} must be greater than 1 and less than 179");
    }
    Ok(())
}

fn validate_unit_ratio(value: f32, name: &str) -> Result<()> {
    if !(value.is_finite() && (0.0..=1.0).contains(&value)) {
        bail!("{name} must be in [0.0, 1.0]");
    }
    Ok(())
}
