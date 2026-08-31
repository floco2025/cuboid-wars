use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

use super::state::SettingsMenuState;
use crate::ui::ConsoleState;

// Esc toggles the settings overlay and with it the cursor. The console has
// Esc priority: its input system runs just before this one, so an Esc that
// closed the console leaves `ConsoleState` marked changed and is swallowed.
pub(super) fn settings_menu_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    console: Res<ConsoleState>,
    mut menu: ResMut<SettingsMenuState>,
    mut cursor_options: Single<&mut CursorOptions>,
) {
    if !keyboard.just_pressed(KeyCode::Escape) {
        return;
    }
    if console.open || console.is_changed() {
        return;
    }
    menu.open = !menu.open;
    cursor_options.visible = menu.open;
    cursor_options.grab_mode = if menu.open {
        CursorGrabMode::None
    } else {
        CursorGrabMode::Locked
    };
}
