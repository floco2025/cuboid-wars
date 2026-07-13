use std::cmp::Reverse;

use bevy::{
    input::mouse::MouseButton,
    prelude::*,
    window::{
        CursorOptions, Monitor, MonitorSelection, OnMonitor, PrimaryMonitor, PrimaryWindow, VideoMode,
        VideoModeSelection, WindowMode,
    },
};

use crate::{
    cameras::{CameraViewMode, TopDownCameraYaw},
    config::ClientSettings,
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

// Toggle fullscreen with Cmd/Ctrl+F or F11
pub fn input_fullscreen_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    settings: Res<ClientSettings>,
    mut windows: Query<(&mut Window, Option<&OnMonitor>), With<PrimaryWindow>>,
    monitors: Query<(Entity, &Monitor, Has<PrimaryMonitor>)>,
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

    let current_monitor = on_monitor
        .and_then(|on_monitor| monitors.get(on_monitor.0).ok())
        .map(|(entity, monitor, _)| (entity, monitor));
    let primary_monitor = || {
        monitors
            .iter()
            .find_map(|(entity, monitor, is_primary)| is_primary.then_some((entity, monitor)))
    };
    let Some((monitor_entity, monitor)) = current_monitor.or_else(primary_monitor) else {
        warn!("cannot enter exclusive fullscreen because no monitor is available");
        return;
    };

    let fullscreen = settings.rendering.exclusive_fullscreen;
    let Some(video_mode) = select_exclusive_video_mode(&monitor.video_modes, fullscreen.width, fullscreen.height)
    else {
        warn!("cannot enter exclusive fullscreen because the monitor reports no video modes");
        return;
    };
    if video_mode.physical_size != UVec2::new(fullscreen.width, fullscreen.height) {
        warn!(
            "exclusive fullscreen resolution {}x{} is unavailable; using {}x{}",
            fullscreen.width, fullscreen.height, video_mode.physical_size.x, video_mode.physical_size.y
        );
    }
    window.mode = WindowMode::Fullscreen(
        MonitorSelection::Entity(monitor_entity),
        VideoModeSelection::Specific(video_mode),
    );
}

fn select_exclusive_video_mode(modes: &[VideoMode], width: u32, height: u32) -> Option<VideoMode> {
    modes.iter().copied().min_by_key(|mode| {
        let width_error = u64::from(mode.physical_size.x.abs_diff(width));
        let height_error = u64::from(mode.physical_size.y.abs_diff(height));
        (
            width_error * width_error + height_error * height_error,
            Reverse(mode.refresh_rate_millihertz),
            Reverse(mode.bit_depth),
        )
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn video_mode(width: u32, height: u32, refresh_rate_millihertz: u32, bit_depth: u16) -> VideoMode {
        VideoMode {
            physical_size: UVec2::new(width, height),
            refresh_rate_millihertz,
            bit_depth,
        }
    }

    #[test]
    fn exclusive_video_mode_prefers_exact_resolution_and_highest_refresh_rate() {
        let modes = [
            video_mode(3840, 2160, 60_000, 30),
            video_mode(2560, 1440, 60_000, 30),
            video_mode(2560, 1440, 120_000, 24),
        ];

        assert_eq!(
            select_exclusive_video_mode(&modes, 2560, 1440),
            Some(video_mode(2560, 1440, 120_000, 24))
        );
    }

    #[test]
    fn exclusive_video_mode_uses_closest_supported_resolution() {
        let modes = [
            video_mode(3840, 2160, 60_000, 30),
            video_mode(1920, 1080, 60_000, 30),
            video_mode(1280, 720, 60_000, 30),
        ];

        assert_eq!(
            select_exclusive_video_mode(&modes, 2560, 1440),
            Some(video_mode(1920, 1080, 60_000, 30))
        );
    }

    #[test]
    fn exclusive_video_mode_returns_none_without_supported_modes() {
        assert_eq!(select_exclusive_video_mode(&[], 2560, 1440), None);
    }
}
