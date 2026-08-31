use bevy::{
    input::mouse::MouseButton,
    prelude::*,
    window::{CursorOptions, Monitor, MonitorSelection, OnMonitor, PrimaryMonitor, PrimaryWindow, WindowMode},
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
pub fn input_debug_colors_cycle_system(keyboard: Res<ButtonInput<KeyCode>>, mut debug_colors: ResMut<DebugColors>) {
    if keyboard.just_pressed(KeyCode::KeyC) {
        debug_colors.0 = debug_colors.0.next();
    }
}

// Toggle fullscreen with Cmd/Ctrl+F or F11. Borderless on every platform:
// the render-resolution cap (`scene_render_target_system`) supplies the
// lower-resolution rendering that exclusive fullscreen used to.
pub fn input_fullscreen_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut windows: Query<(&mut Window, Option<&OnMonitor>), With<PrimaryWindow>>,
    monitors: Query<(Entity, Has<PrimaryMonitor>), With<Monitor>>,
) {
    let cmd_held = keyboard.pressed(KeyCode::SuperLeft) || keyboard.pressed(KeyCode::SuperRight);
    let ctrl_held = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let f_pressed = keyboard.just_pressed(KeyCode::KeyF);
    let f11_pressed = keyboard.just_pressed(KeyCode::F11);

    if !(((cmd_held || ctrl_held) && f_pressed) || f11_pressed) {
        return;
    }
    let Ok((mut window, on_monitor)) = windows.single_mut() else {
        return;
    };
    if !matches!(window.mode, WindowMode::Windowed) {
        window.mode = WindowMode::Windowed;
        return;
    }
    enter_borderless_fullscreen(&mut window, on_monitor, &monitors);
}

// Enter borderless fullscreen on the window's current monitor (primary as
// the fallback). Shared by the Cmd/Ctrl+F toggle and the settings menu.
pub fn enter_borderless_fullscreen(
    window: &mut Window,
    on_monitor: Option<&OnMonitor>,
    monitors: &Query<(Entity, Has<PrimaryMonitor>), With<Monitor>>,
) {
    let current_monitor = on_monitor
        .map(|on_monitor| on_monitor.0)
        .filter(|&entity| monitors.contains(entity));
    let primary_monitor = || {
        monitors
            .iter()
            .find_map(|(entity, is_primary)| is_primary.then_some(entity))
    };
    let Some(monitor_entity) = current_monitor.or_else(primary_monitor) else {
        warn!("cannot enter fullscreen because no monitor is available");
        return;
    };
    window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Entity(monitor_entity));
}

// Any left click while the cursor is free re-locks it. Esc and the cursor
// belong to the settings menu (`settings_menu_toggle_system`); this system
// is gated off while the menu is open so widget clicks don't re-lock. The
// click is deliberately not consumed — it still fires the shooting system.
pub fn input_cursor_toggle_system(
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor_options: Single<&mut CursorOptions>,
) {
    if mouse.just_pressed(MouseButton::Left) && cursor_options.visible {
        cursor_options.visible = false;
        cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
    }
}
