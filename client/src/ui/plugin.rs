use super::*;
use bevy::prelude::*;

use crate::{
    players::death_overlay_visibility_system, schedule::ClientSet,
    ui::floating_labels::floating_label_scale_compensation_system,
};

// HUD and screen-space UI. The `Hud` set runs after `Input` so the console
// renders this frame's post-keystroke state.
pub fn hud_plugin(app: &mut App) {
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
            ui_stunned_blink_system,
            ui_rtt_system,
            ui_fps_system,
            ui_crosshair_lock_system,
            death_overlay_visibility_system,
            render_pending_messages_system,
            update_message_feed_system,
            hud_banner_system,
            console_render_system,
        )
            .in_set(ClientSet::Hud),
    );
}
