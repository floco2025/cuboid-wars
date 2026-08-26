mod console;
mod crosshair;
mod diagnostics;
mod fade;
pub mod floating_labels;
mod hud_banner;
mod message_feed;
mod player_list;
mod quest_panel;
mod scale;
mod setup;

pub use console::{ConsoleState, console_input_system, console_render_system, spawn_console};
pub use crosshair::{CrosshairBarMarker, CrosshairMarker, ui_crosshair_lock_system, ui_crosshair_visibility_system};
pub use diagnostics::{FpsMarker, FpsMeasurement, RttMarker, ui_fps_system, ui_rtt_system};
pub use fade::fade_out_alpha;
pub use hud_banner::{PendingBanner, render_pending_banner_system, tick_hud_banner_system};
pub use message_feed::{GameMessageFeed, render_pending_messages_system, update_message_feed_system};
pub use player_list::{
    HudShapeAssets, ui_health_bar_fill_system, ui_player_list_rebuild_system, ui_stunned_blink_system,
};
pub use quest_panel::{QuestEntry, QuestLog, ui_quest_panel_rebuild_system};
pub use scale::ui_hud_scale_system;
pub use setup::{DeathOverlayMarker, setup_ui_system};

mod plugin;

pub use plugin::hud_plugin;
