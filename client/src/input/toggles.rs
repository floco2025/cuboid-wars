use bevy::{
    prelude::*,
    window::{Monitor, MonitorSelection, OnMonitor, PrimaryMonitor, PrimaryWindow, WindowMode, WindowPosition},
};

use crate::{
    cameras::{CameraViewMode, TopDownCameraYaw},
    map::{DebugColors, LevelFocusEnabled},
    players::LocalPlayerInfo,
};

// The window's placement while windowed and, in fullscreen, the last one.
// Saved with the local settings and seeded from them at startup; leaving
// fullscreen requests it back, since a window created fullscreen has no
// windowed frame for the OS to restore. The position is logical points, not
// physical pixels: macOS positions in points, so a position saved at one
// display scale restores correctly at another. `None` lets the OS place it.
#[derive(Resource, Clone, Copy)]
pub struct WindowedFrame {
    pub position: Option<IVec2>,
    pub size: UVec2,
    // A restored position is applied one windowed frame late, not at window
    // creation: on macOS creation places the content and runtime placement
    // the frame, and only the latter round-trips a recorded position. At
    // startup the window is created hidden until then, so the move is unseen.
    pub position_pending: bool,
}

// Runs in `PreUpdate`, so a pending position always follows the previous
// frame's size request; otherwise records the placement while windowed.
// `Window.position` is physical, so it converts through the current scale.
pub fn windowed_frame_system(mut windows: Query<&mut Window, With<PrimaryWindow>>, mut frame: ResMut<WindowedFrame>) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    if !matches!(window.mode, WindowMode::Windowed) {
        return;
    }
    let scale = window.resolution.scale_factor();
    if frame.position_pending {
        frame.position_pending = false;
        if let Some(logical) = frame.position {
            let physical = (logical.as_vec2() * scale).round().as_ivec2();
            window.position = WindowPosition::At(physical);
        }
        return;
    }
    // Reveal the frame after the position landed, so a hidden-start window
    // never flashes at its creation spot.
    if !window.visible {
        window.visible = true;
    }
    let size = window.size().round().as_uvec2();
    // A minimized window reports 0x0 on some platforms.
    if !size.cmpgt(UVec2::ZERO).all() {
        return;
    }
    frame.size = size;
    if let WindowPosition::At(physical) = window.position {
        frame.position = Some((physical.as_vec2() / scale).round().as_ivec2());
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
    mut frame: ResMut<WindowedFrame>,
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
