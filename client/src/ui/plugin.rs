use super::console::{ConsoleSubmission, console_input_system, console_send_system};
use super::*;
use bevy::prelude::*;

use crate::{
    players::death_overlay_visibility_system, schedule::ClientSet,
    ui::floating_labels::floating_label_scale_compensation_system,
};

// HUD and screen-space UI (`ClientSet::Hud`), plus the console's keystroke
// system in its own earlier set.
pub fn hud_plugin(app: &mut App) {
    app.add_plugins(settings_menu_plugin);
    app.add_message::<ConsoleSubmission>().add_systems(
        Update,
        (console_input_system, console_send_system)
            .chain()
            .in_set(ClientSet::Console)
            .run_if(menu_closed),
    );
    app.add_systems(
        Update,
        (
            ui_hud_scale_system,
            // Cancels the HUD scale inside the fixed-size label textures;
            // must observe this frame's scale, not last frame's.
            floating_label_scale_compensation_system.after(ui_hud_scale_system),
            ui_crosshair_visibility_system,
            ui_player_list_rebuild_system,
            ui_health_bar_fill_system.after(ui_player_list_rebuild_system),
            ui_quest_panel_rebuild_system,
            ui_quest_panel_offset_system,
            ui_stunned_blink_system,
            ui_rtt_system,
            ui_fps_system,
            ui_crosshair_system,
            ui_crosshair_lock_system.after(ui_crosshair_system),
            death_overlay_visibility_system,
            ui_message_feed_system,
            ui_hud_banner_system,
            // After the spawners, so a burst is capped before it ever renders.
            ui_timed_lines_system
                .after(ui_message_feed_system)
                .after(ui_hud_banner_system),
            ui_console_render_system,
            ui_diagnostics_visibility_system,
        )
            .in_set(ClientSet::Hud),
    );
}
