use bevy::{
    prelude::*,
    window::{Monitor, MonitorSelection, OnMonitor, PrimaryMonitor, PrimaryWindow, WindowMode},
};

use super::{WindowedFrame, camera::fullscreen_shortcut_modifier};

use crate::{
    cameras::{CameraViewMode, FollowCamera, TopDownCameraYaw},
    characters::BoundsMode,
    map::{DebugColors, LevelFocusEnabled},
    players::LocalPlayerInfo,
};

pub fn input_bounds_cycle_system(keyboard: Res<ButtonInput<KeyCode>>, mut visible: ResMut<BoundsMode>) {
    if keyboard.just_pressed(KeyCode::KeyB) {
        *visible = visible.next();
    }
}

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
    mut third: ResMut<FollowCamera>,
) {
    if keyboard.just_pressed(KeyCode::KeyV) {
        let new_mode = third.toggle_top_down(*view_mode);
        *view_mode = new_mode;

        if new_mode.is_top_down() {
            top_down_camera_yaw.0 = local_player_info.stored_yaw;
        }
        focus.0 = new_mode.is_top_down();
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
    mut frame: ResMut<WindowedFrame>,
    mut windows: Query<(&mut Window, Option<&OnMonitor>), With<PrimaryWindow>>,
    monitors: Query<(Entity, Has<PrimaryMonitor>), With<Monitor>>,
) {
    let f_pressed = keyboard.just_pressed(KeyCode::KeyF);
    let f11_pressed = keyboard.just_pressed(KeyCode::F11);

    if !((fullscreen_shortcut_modifier(&keyboard) && f_pressed) || f11_pressed) {
        return;
    }
    let Ok((mut window, on_monitor)) = windows.single_mut() else {
        return;
    };
    if !matches!(window.mode, WindowMode::Windowed) {
        enter_windowed(&mut window, &mut frame);
        return;
    }
    enter_borderless_fullscreen(&mut window, on_monitor, &monitors);
}

pub fn enter_windowed(window: &mut Window, frame: &mut WindowedFrame) {
    window.mode = WindowMode::Windowed;
    let size = frame.size.as_vec2();
    window.resolution.set(size.x, size.y);
    frame.position_pending = true;
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
