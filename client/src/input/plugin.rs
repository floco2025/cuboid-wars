use super::*;
use bevy::prelude::*;

use crate::{schedule::ClientSet, ui::console_input_system};

// Input writes local intent and view/debug state.
pub fn input_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            // Console open/close must land before every gated input
            // system, so a keystroke can't both type and act in-game.
            console_input_system
                .before(input_cursor_toggle_system)
                .before(input_camera_view_toggle_system)
                .before(input_level_focus_toggle_system)
                .before(input_debug_colors_cycle_system),
            input_movement_system.after(input_camera_view_toggle_system),
            input_shooting_system.after(input_movement_system),
            input_missile_system.after(input_movement_system),
            // Run before shooting (which is after movement) so a click that
            // re-locks the cursor also fires that same frame, instead of
            // depending on nondeterministic system order.
            input_cursor_toggle_system.before(input_movement_system),
            input_camera_view_toggle_system,
            input_level_focus_toggle_system,
            input_fullscreen_toggle_system,
            input_debug_colors_cycle_system,
        )
            .in_set(ClientSet::Input),
    );
}
