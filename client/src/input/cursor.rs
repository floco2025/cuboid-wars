use crate::{
    cameras::CameraInputState,
    ui::{ConsoleState, SettingsMenuState},
};
use bevy::{
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow, WindowFocused},
};

pub fn input_cursor_capture_system(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut focus: MessageReader<WindowFocused>,
    mut motion: MessageReader<MouseMotion>,
    mut discard_next_motion: Local<bool>,
    console: Res<ConsoleState>,
    menu: Res<SettingsMenuState>,
    mut input: ResMut<CameraInputState>,
    window: Single<(Entity, &mut Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let (entity, mut window, mut cursor) = window.into_inner();
    let mut mouse_delta = Vec2::ZERO;
    for event in motion.read().filter(|event| event.delta != Vec2::ZERO) {
        if *discard_next_motion {
            *discard_next_motion = false;
        } else {
            mouse_delta += event.delta;
        }
    }
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
        if window.cursor_position() != Some(centre) {
            window.set_cursor_position(Some(centre));
            // macOS folds a warp into the next motion event, even after idle frames.
            *discard_next_motion = cfg!(target_os = "macos");
        }
    }
    input.mouse_delta = if capture && window.focused && !regrab {
        mouse_delta
    } else {
        Vec2::ZERO
    };
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
