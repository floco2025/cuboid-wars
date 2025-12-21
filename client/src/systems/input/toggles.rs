use bevy::{
    input::mouse::MouseButton,
    prelude::*,
    window::CursorOptions,
};

use crate::resources::{CameraViewMode, RoofRenderingEnabled};

// ============================================================================
// Input Toggle Systems
// ============================================================================

// Toggle camera view mode with V key
pub fn input_camera_view_toggle_system(keyboard: Res<ButtonInput<KeyCode>>, mut view_mode: ResMut<CameraViewMode>) {
    if keyboard.just_pressed(KeyCode::KeyV) {
        *view_mode = match *view_mode {
            CameraViewMode::FirstPerson => CameraViewMode::TopDown,
            CameraViewMode::TopDown => CameraViewMode::FirstPerson,
        };
    }
}

// Toggle roof rendering with R key
pub fn input_roof_toggle_system(keyboard: Res<ButtonInput<KeyCode>>, mut roof_enabled: ResMut<RoofRenderingEnabled>) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        roof_enabled.0 = !roof_enabled.0;
    }
}

// Toggle cursor lock with Escape key or mouse click
pub fn input_cursor_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor_options: Single<&mut CursorOptions>,
) {
    // Escape key toggles cursor lock
    if keyboard.just_pressed(KeyCode::Escape) {
        cursor_options.visible = !cursor_options.visible;
        cursor_options.grab_mode = if cursor_options.visible {
            bevy::window::CursorGrabMode::None
        } else {
            bevy::window::CursorGrabMode::Locked
        };
    }

    // Left click locks cursor if it's currently unlocked
    // Don't consume the click - let it pass through to shooting system
    if mouse.just_pressed(bevy::input::mouse::MouseButton::Left) && cursor_options.visible {
        cursor_options.visible = false;
        cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
        // Note: The click event will still be available for the shooting system
    }
}
