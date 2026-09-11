use crate::{
    cameras::CameraInputState,
    ui::{ConsoleState, SettingsMenuState},
};
use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow, WindowFocused},
};

pub fn input_cursor_capture_system(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut focus: MessageReader<WindowFocused>,
    console: Res<ConsoleState>,
    menu: Res<SettingsMenuState>,
    mut input: ResMut<CameraInputState>,
    window: Single<(Entity, &mut Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let (entity, mut window, mut cursor) = window.into_inner();
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
    // macOS locks the pointer only for the foreground app and leaves it where
    // it was, so a window that just became key centres the pointer and grabs
    // again; winit's warp re-associates the mouse, so the grab must follow it.
    let focused = focus
        .read()
        .filter(|event| event.window == entity)
        .last()
        .is_some_and(|event| event.focused);
    let regrab = capture && focused;
    if regrab {
        let centre = window.size() / 2.0;
        window.set_cursor_position(Some(centre));
    }
    // Equal writes would send redundant cursor updates to winit; a regrab
    // wants exactly that write, since Bevy re-grabs on any change.
    if cursor.visible == capture {
        cursor.visible = !capture;
    }
    if cursor.grab_mode != grab_mode || regrab {
        cursor.grab_mode = grab_mode;
    }
}

#[cfg(test)]
#[path = "tests/cursor.rs"]
mod tests;
