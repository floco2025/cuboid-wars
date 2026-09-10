use super::{focus::input_focus_system, *};
use bevy::{input::InputSystems, prelude::*};

use crate::{
    missiles::lock_on_system,
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
    // Clear held input and intent before FixedUpdate can simulate another step.
    app.add_systems(PreUpdate, input_focus_system.after(InputSystems));
    app.add_systems(PreUpdate, windowed_frame_system);
    app.add_systems(
        Update,
        (
            input_movement_system
                .after(input_cursor_capture_system)
                .after(input_camera_zoom_system),
            input_cursor_capture_system.after(input_camera_view_toggle_system),
            // Zooming in locks the facing, which movement reads this frame.
            input_camera_zoom_system
                .after(input_facing_lock_toggle_system)
                .after(input_cursor_capture_system),
            input_weapon_select_system
                .after(input_movement_system)
                .after(ClientSet::Network),
            // The fullscreen shortcut works with the settings menu open, only
            // the console (which the F key types into) stands it down.
            input_fullscreen_toggle_system.run_if(console_closed),
            (
                input_camera_view_toggle_system,
                input_facing_lock_toggle_system,
                input_level_focus_toggle_system,
                input_debug_colors_cycle_system,
                input_bounds_cycle_system,
            )
                .run_if(gameplay_input_active),
        )
            .in_set(ClientSet::Input),
    );
    app.add_systems(
        Update,
        (input_shooting_system, input_missile_system, input_portal_system)
            .in_set(ClientSet::Camera)
            .after(lock_on_system)
            .run_if(gameplay_input_active),
    );
}
