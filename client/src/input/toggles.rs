use bevy::{
    input::mouse::MouseButton,
    prelude::*,
    window::{CursorOptions, MonitorSelection, WindowMode},
};

use crate::{
    cameras::{CameraViewMode, TopDownCameraYaw},
    map::{DebugColors, LevelFocusEnabled},
    players::LocalPlayerInfo,
};

// ============================================================================
// Input Toggle Systems
// ============================================================================

// Toggle camera view mode with V key
pub fn input_camera_view_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut view_mode: ResMut<CameraViewMode>,
    mut focus: ResMut<LevelFocusEnabled>,
    mut top_down_camera_yaw: ResMut<TopDownCameraYaw>,
    local_player_info: Res<LocalPlayerInfo>,
) {
    if keyboard.just_pressed(KeyCode::KeyV) {
        let old_mode = *view_mode;
        let new_mode = old_mode.next();
        *view_mode = new_mode;

        if old_mode.is_first_person() && new_mode.is_top_down() {
            top_down_camera_yaw.0 = local_player_info.stored_yaw;
            focus.0 = true;
        } else if old_mode.is_top_down() && new_mode.is_first_person() {
            focus.0 = false;
        }
    }
}

// Toggle level-focus mode with R key. When enabled, the visibility system
// hides walls/floors at other levels and ramps that don't touch the local
// player's current level.
pub fn input_level_focus_toggle_system(keyboard: Res<ButtonInput<KeyCode>>, mut focus: ResMut<LevelFocusEnabled>) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        focus.0 = !focus.0;
    }
}

// Cycle the map's debug-color mode with C key: Off → ByMaterial → BySegment → Off.
// The map geometry respawns automatically (see `map_spawn_geometry_system`).
pub fn input_debug_colors_cycle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut debug_colors: ResMut<DebugColors>,
) {
    if keyboard.just_pressed(KeyCode::KeyC) {
        debug_colors.0 = debug_colors.0.next();
    }
}

// Toggle fullscreen with Cmd/Ctrl+F or F11
pub fn input_fullscreen_toggle_system(keyboard: Res<ButtonInput<KeyCode>>, mut window: Single<&mut Window>) {
    let cmd_held = keyboard.pressed(KeyCode::SuperLeft) || keyboard.pressed(KeyCode::SuperRight);
    let ctrl_held = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let f_pressed = keyboard.just_pressed(KeyCode::KeyF);
    let f11_pressed = keyboard.just_pressed(KeyCode::F11);

    if ((cmd_held || ctrl_held) && f_pressed) || f11_pressed {
        window.mode = match window.mode {
            WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            _ => WindowMode::Windowed,
        };
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
