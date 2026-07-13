use anyhow::{Result, bail};
use serde::Deserialize;

use super::settings::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Deserialize)]
pub struct HudConfig {
    // Window width (logical px) the configured HUD sizes are designed for.
    // The whole screen-space HUD scales by window_width / reference_width.
    #[serde(default = "default_hud_reference_width")]
    pub reference_width: f32,
    pub font_sizes: FontSizesConfig,
    pub message_feed: MessageFeedConfig,
    pub floating_labels: FloatingLabelsConfig,
    pub health_bars: HealthBarsConfig,
    pub banner: BannerConfig,
    pub quest_panel: QuestPanelConfig,
    pub death_overlay: DeathOverlayConfig,
}

const fn default_hud_reference_width() -> f32 {
    1280.0
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
    // Centered HUD banner ("Collect 10 Gold!", "You died!", etc.).
    pub banner: f32,
    // Quest-panel rows (top-right): title + progress counter.
    pub quest_panel: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MessageFeedConfig {
    pub entry_duration_secs: f32,
    pub max_entries: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FloatingLabelsConfig {
    pub height_above_character: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct HealthBarsConfig {
    // Height ÷ width of the world-space bar above a character. The width is
    // fixed (`LABEL_PLAYER_BAR_WIDTH` / `LABEL_ACTOR_MESH_WIDTH`, meters);
    // only the proportions are tunable.
    pub floating_player_aspect: f32,
    pub floating_actor_aspect: f32,
    // On-screen bar under each player-list row, in logical px.
    pub player_list_width: f32,
    pub player_list_height: f32,
}

// Per-quest progress bar in the top-right quest panel (colors are consts in
// `constants.rs`).
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct QuestPanelConfig {
    pub bar_width: f32,
    pub bar_height: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BannerConfig {
    pub quest_announcement_duration_secs: f32,
    pub quest_completed_duration_secs: f32,
    pub death_duration_secs: f32,
    pub death_text: String,
    pub fade_duration_secs: f32,
}

// Red full-screen death tint. Timer-driven (no fade in): snaps on at
// peak alpha when the player dies, holds, and fades out over the final
// `fade_duration_secs` before disappearing at `duration_secs`. Peak
// alpha is currently a const (`DEATH_OVERLAY_MAX_ALPHA` in
// `players/death.rs`); expose here if it ever needs tuning.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct DeathOverlayConfig {
    pub duration_secs: f32,
    pub fade_duration_secs: f32,
}

impl HudConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_positive_finite(self.reference_width, "hud.reference_width")?;
        validate_positive_finite(self.font_sizes.player_list, "hud.font_sizes.player_list")?;
        validate_positive_finite(self.font_sizes.score, "hud.font_sizes.score")?;
        validate_positive_finite(self.font_sizes.message_feed, "hud.font_sizes.message_feed")?;
        validate_positive_finite(self.font_sizes.floating_label, "hud.font_sizes.floating_label")?;
        validate_positive_finite(self.font_sizes.banner, "hud.font_sizes.banner")?;
        validate_positive_finite(self.font_sizes.quest_panel, "hud.font_sizes.quest_panel")?;
        validate_non_negative_finite(
            self.message_feed.entry_duration_secs,
            "hud.message_feed.entry_duration_secs",
        )?;
        if self.message_feed.max_entries == 0 {
            bail!("hud.message_feed.max_entries must be > 0");
        }
        validate_positive_finite(
            self.floating_labels.height_above_character,
            "hud.floating_labels.height_above_character",
        )?;
        validate_positive_finite(
            self.health_bars.floating_player_aspect,
            "hud.health_bars.floating_player_aspect",
        )?;
        validate_positive_finite(
            self.health_bars.floating_actor_aspect,
            "hud.health_bars.floating_actor_aspect",
        )?;
        validate_positive_finite(self.health_bars.player_list_width, "hud.health_bars.player_list_width")?;
        validate_positive_finite(
            self.health_bars.player_list_height,
            "hud.health_bars.player_list_height",
        )?;
        validate_positive_finite(
            self.banner.quest_announcement_duration_secs,
            "hud.banner.quest_announcement_duration_secs",
        )?;
        validate_positive_finite(
            self.banner.quest_completed_duration_secs,
            "hud.banner.quest_completed_duration_secs",
        )?;
        validate_positive_finite(self.banner.death_duration_secs, "hud.banner.death_duration_secs")?;
        if self.banner.death_text.is_empty() {
            bail!("hud.banner.death_text must not be empty");
        }
        validate_positive_finite(self.banner.fade_duration_secs, "hud.banner.fade_duration_secs")?;
        validate_positive_finite(self.quest_panel.bar_width, "hud.quest_panel.bar_width")?;
        validate_positive_finite(self.quest_panel.bar_height, "hud.quest_panel.bar_height")?;
        validate_positive_finite(self.death_overlay.duration_secs, "hud.death_overlay.duration_secs")?;
        validate_positive_finite(
            self.death_overlay.fade_duration_secs,
            "hud.death_overlay.fade_duration_secs",
        )?;
        Ok(())
    }
}
