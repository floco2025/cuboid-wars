use bevy::audio::GlobalVolume;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowMode};

use super::state::SettingsMenuState;
use crate::config::{ClientSettings, LOCAL_SETTINGS_VERSION, LocalSettings};

// Closing the menu is the commit point: the panel's values are written to
// `client_local.json` and restored at the next launch.
pub(super) fn save_local_settings_system(
    menu: Res<SettingsMenuState>,
    settings: Res<ClientSettings>,
    global_volume: Res<GlobalVolume>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut was_open: Local<bool>,
) {
    let closed_now = *was_open && !menu.open;
    *was_open = menu.open;
    if !closed_now {
        return;
    }
    let fullscreen = windows
        .single()
        .is_ok_and(|window| !matches!(window.mode, WindowMode::Windowed));
    let local = LocalSettings {
        version: LOCAL_SETTINGS_VERSION,
        fullscreen,
        fullscreen_resolution: settings.rendering.fullscreen_resolution,
        vsync: settings.rendering.vsync,
        msaa_samples: settings.rendering.msaa_samples,
        mouse_sensitivity: settings.input.mouse_sensitivity,
        invert_y: settings.input.invert_y,
        fov_degrees: settings.camera.fov_degrees.first_person,
        shake_scale: settings.camera.shake.scale,
        master_volume: global_volume.volume.to_linear(),
        show_diagnostics: settings.hud.show_diagnostics,
        rearview_mirror: settings.camera.rearview.enabled,
    };
    if let Err(error) = local.save() {
        warn!("failed to save settings: {error:#}");
    }
}
