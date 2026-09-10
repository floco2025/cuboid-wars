use crate::{
    cameras::CameraInputState,
    ui::{ConsoleState, SettingsMenuState},
};
use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

pub fn input_cursor_capture_system(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    console: Res<ConsoleState>,
    menu: Res<SettingsMenuState>,
    mut input: ResMut<CameraInputState>,
    mut cursor: Single<&mut CursorOptions>,
) {
    input.suppress_fire = false;
    let overlay = console.open || menu.open;
    if input.released
        && !overlay
        && !keyboard.just_pressed(KeyCode::Escape)
        && mouse.get_just_pressed().next().is_some()
    {
        input.released = false;
        input.suppress_fire = true;
    }
    let capture = !overlay && !input.released;
    let grab_mode = if capture {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    // Equal writes would send redundant cursor updates to winit.
    if cursor.visible == capture {
        cursor.visible = !capture;
    }
    if cursor.grab_mode != grab_mode {
        cursor.grab_mode = grab_mode;
    }
}

#[cfg(test)]
#[path = "tests/cursor.rs"]
mod tests;
