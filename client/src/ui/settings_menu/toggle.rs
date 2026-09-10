use bevy::prelude::*;

use super::state::SettingsMenuState;
use crate::{cameras::CameraInputState, ui::ConsoleState};

// Shift+Esc releases the cursor without opening the overlay. Plain Esc toggles
// the overlay and with it the cursor. The console has Esc priority: its input
// system runs just before this one, so an Esc that closed the console leaves
// `ConsoleState` marked changed and is swallowed.
pub(super) fn settings_menu_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    console: Res<ConsoleState>,
    mut menu: ResMut<SettingsMenuState>,
    mut input_state: ResMut<CameraInputState>,
) {
    if !keyboard.just_pressed(KeyCode::Escape) {
        return;
    }
    if console.open || console.is_changed() {
        return;
    }
    let shift_pressed = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    if shift_pressed {
        input_state.released = true;
        return;
    }
    menu.open = !menu.open;
    input_state.released = false;
}

#[cfg(test)]
#[path = "tests/toggle.rs"]
mod tests;
