use super::*;
use bevy::prelude::*;

use crate::{
    schedule::ClientSet,
    ui::{ConsoleState, SettingsMenuState},
};

// Gameplay input stands down while a text or menu overlay is open; movement
// and weapon selection keep their own checks because they must still idle
// the intent, drain the mouse, and re-select an expired weapon.
fn gameplay_input_active(console: Res<ConsoleState>, menu: Res<SettingsMenuState>) -> bool {
    !console.open && !menu.open
}

pub fn input_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            input_movement_system.after(input_camera_view_toggle_system),
            input_weapon_select_system.after(input_movement_system),
            (
                input_shooting_system.after(input_weapon_select_system),
                input_missile_system.after(input_weapon_select_system),
                input_portal_system.after(input_weapon_select_system),
                input_cursor_capture_system
                    .after(input_shooting_system)
                    .after(input_missile_system)
                    .after(input_portal_system),
                input_camera_view_toggle_system,
                input_level_focus_toggle_system,
                input_fullscreen_toggle_system,
                input_debug_colors_cycle_system,
            )
                .run_if(gameplay_input_active),
        )
            .in_set(ClientSet::Input),
    );
}
