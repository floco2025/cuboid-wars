use super::*;
use bevy::prelude::*;

use crate::{
    schedule::ClientSet,
    ui::{ConsoleState, SettingsMenuState, console_closed},
};

// Gameplay input stands down while a text or menu overlay is open; movement
// and weapon selection keep their own checks because they must still idle
// the intent, drain the mouse, and re-select an expired weapon.
fn gameplay_input_active(console: Res<ConsoleState>, menu: Res<SettingsMenuState>) -> bool {
    !console.open && !menu.open
}

pub fn input_plugin(app: &mut App) {
    app.init_resource::<PendingWeaponSelection>();
    app.add_systems(PreUpdate, windowed_frame_system);
    app.add_systems(
        Update,
        (
            input_movement_system.after(input_camera_view_toggle_system),
            input_weapon_select_system
                .after(input_movement_system)
                .after(ClientSet::Network),
            // The fullscreen shortcut works with the settings menu open, only
            // the console (which the F key types into) stands it down.
            input_fullscreen_toggle_system.run_if(console_closed),
            (
                input_shooting_system.after(input_weapon_select_system),
                input_missile_system.after(input_weapon_select_system),
                // Aim needs matching plate state and bridge collider groups.
                input_portal_system
                    .after(input_weapon_select_system)
                    .after(ClientSet::Network),
                input_cursor_capture_system
                    .after(input_shooting_system)
                    .after(input_missile_system)
                    .after(input_portal_system),
                input_camera_view_toggle_system,
                input_level_focus_toggle_system,
                input_debug_colors_cycle_system,
                input_collider_boxes_toggle_system,
            )
                .run_if(gameplay_input_active),
        )
            .in_set(ClientSet::Input),
    );
}
