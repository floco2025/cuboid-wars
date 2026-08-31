use super::*;
use bevy::prelude::*;

use crate::{
    schedule::ClientSet,
    ui::{console_closed, menu_closed},
};

// Input writes local intent and view/debug state. Everything but movement
// stands down while the console or the settings menu is open; movement keeps
// its own check because it must still idle the intent and drain the mouse.
pub fn input_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            input_movement_system.after(input_camera_view_toggle_system),
            input_shooting_system
                .after(input_movement_system)
                .run_if(console_closed)
                .run_if(menu_closed),
            input_missile_system
                .after(input_movement_system)
                .run_if(console_closed)
                .run_if(menu_closed),
            // Run before shooting (which is after movement) so a click that
            // re-locks the cursor also fires that same frame, instead of
            // depending on nondeterministic system order.
            input_cursor_toggle_system
                .before(input_movement_system)
                .run_if(console_closed)
                .run_if(menu_closed),
            input_camera_view_toggle_system
                .run_if(console_closed)
                .run_if(menu_closed),
            input_level_focus_toggle_system
                .run_if(console_closed)
                .run_if(menu_closed),
            input_fullscreen_toggle_system
                .run_if(console_closed)
                .run_if(menu_closed),
            input_debug_colors_cycle_system
                .run_if(console_closed)
                .run_if(menu_closed),
        )
            .in_set(ClientSet::Input),
    );
}
