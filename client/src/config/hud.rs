use anyhow::{Result, bail};
use serde::Deserialize;

use super::settings::validate_positive_finite;

#[derive(Debug, Clone, Deserialize)]
pub struct HudConfig {
    // Window width (logical px) the configured HUD sizes are designed for.
    // The whole screen-space HUD scales by window_width / reference_width.
    #[serde(default = "default_hud_reference_width")]
    pub reference_width: f32,
    pub font_sizes: FontSizesConfig,
    pub banner: BannerConfig,
    pub message_feed: MessageFeedConfig,
    pub floating_labels: FloatingLabelsConfig,
    pub health_bars: HealthBarsConfig,
    pub quest_panel: QuestPanelConfig,
    // The RTT / FPS readout column (toggleable from the settings menu).
    #[serde(default = "default_true")]
    pub show_diagnostics: bool,
    #[serde(default)]
    pub settings_menu: SettingsMenuHudConfig,
}

const fn default_true() -> bool {
    true
}

const fn default_hud_reference_width() -> f32 {
    1920.0
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
    // Bottom-right message feed lines and the console prompt.
    pub message_feed: f32,
    // Name text inside the 3D floating label above a character.
    pub floating_label: f32,
    // Centered HUD banner ("Collect 10 Gold!", "You died!", etc.).
    pub banner: f32,
    // Quest-panel cards (top-right): title + progress counter.
    pub quest_panel: f32,
    // Settings-menu rows and headers.
    #[serde(default = "default_settings_menu_font_size")]
    pub settings_menu: f32,
}

const fn default_settings_menu_font_size() -> f32 {
    18.0
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MessageFeedConfig {
    pub entry_duration_secs: f32,
    pub max_entries: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BannerConfig {
    pub quest_announcement_secs: f32,
    pub quest_completed_secs: f32,
    pub death_secs: f32,
    pub max_entries: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FloatingLabelsConfig {
    pub height_above: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct HealthBarsConfig {
    // Height ÷ width of the world-space bar above a character. The width is
    // fixed (`LABEL_PLAYER_BAR_WIDTH` / `LABEL_ACTOR_MESH_WIDTH`, meters);
    // only the proportions are tunable.
    pub player_aspect: f32,
    pub actor_aspect: f32,
    // On-screen bar under each player-list row, in logical px.
    pub player_list_width: f32,
    pub player_list_height: f32,
}

// Quest cards in the top-right quest panel, in logical px (colors are consts
// in `constants.rs`).
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct QuestPanelConfig {
    // Width of each card (title line, bar, and scope line all span it).
    pub card_width: f32,
    pub bar_height: f32,
}

// Settings-menu panel, in logical px (colors are consts in `constants.rs`).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct SettingsMenuHudConfig {
    pub panel_width: f32,
    // Width of each slider / cycler control.
    pub control_width: f32,
}

impl Default for SettingsMenuHudConfig {
    fn default() -> Self {
        Self {
            panel_width: 380.0,
            control_width: 160.0,
        }
    }
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
        validate_positive_finite(
            self.banner.quest_announcement_secs,
            "hud.banner.quest_announcement_secs",
        )?;
        validate_positive_finite(self.banner.quest_completed_secs, "hud.banner.quest_completed_secs")?;
        validate_positive_finite(self.banner.death_secs, "hud.banner.death_secs")?;
        if self.banner.max_entries == 0 {
            bail!("hud.banner.max_entries must be > 0");
        }
        validate_positive_finite(
            self.message_feed.entry_duration_secs,
            "hud.message_feed.entry_duration_secs",
        )?;
        if self.message_feed.max_entries == 0 {
            bail!("hud.message_feed.max_entries must be > 0");
        }
        validate_positive_finite(self.floating_labels.height_above, "hud.floating_labels.height_above")?;
        validate_positive_finite(self.health_bars.player_aspect, "hud.health_bars.player_aspect")?;
        validate_positive_finite(self.health_bars.actor_aspect, "hud.health_bars.actor_aspect")?;
        validate_positive_finite(self.health_bars.player_list_width, "hud.health_bars.player_list_width")?;
        validate_positive_finite(
            self.health_bars.player_list_height,
            "hud.health_bars.player_list_height",
        )?;
        validate_positive_finite(self.font_sizes.settings_menu, "hud.font_sizes.settings_menu")?;
        validate_positive_finite(self.settings_menu.panel_width, "hud.settings_menu.panel_width")?;
        validate_positive_finite(self.settings_menu.control_width, "hud.settings_menu.control_width")?;
        validate_positive_finite(self.quest_panel.card_width, "hud.quest_panel.card_width")?;
        validate_positive_finite(self.quest_panel.bar_height, "hud.quest_panel.bar_height")?;
        Ok(())
    }
}
