use bevy::{
    audio::GlobalVolume,
    prelude::*,
    window::{PrimaryWindow, WindowMode},
};

use crate::{
    config::{ClientSettings, LOCAL_SETTINGS_VERSION, LocalSettings},
    input::WindowedFrame,
};

// Panel edits, the fullscreen shortcuts, and window moves and resizes all
// reach this the same way: the snapshot is saved the frame after it stops
// changing, so a slider or window drag writes once when it settles and
// quitting right after loses nothing. It saves with the menu open too, so an
// edit made there is never stranded by a quit.
pub(super) fn save_local_settings_system(
    settings: Res<ClientSettings>,
    global_volume: Res<GlobalVolume>,
    frame: Res<WindowedFrame>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut last_saved: Local<Option<LocalSettings>>,
    mut last_seen: Local<Option<LocalSettings>>,
) {
    let fullscreen = windows
        .single()
        .is_ok_and(|window| !matches!(window.mode, WindowMode::Windowed));
    let local = local_settings(&settings, &global_volume, fullscreen, *frame);
    let stable = last_seen.as_ref() == Some(&local);
    if !stable {
        *last_seen = Some(local.clone());
    }
    let Some(previous) = last_saved.as_ref() else {
        *last_saved = Some(local);
        return;
    };
    if !should_save(stable, previous, &local) {
        return;
    }
    if let Err(error) = local.save() {
        warn!("failed to save settings: {error:#}");
    }
    *last_saved = Some(local);
}

fn should_save(stable: bool, previous: &LocalSettings, current: &LocalSettings) -> bool {
    stable && current != previous
}

fn local_settings(
    settings: &ClientSettings,
    global_volume: &GlobalVolume,
    fullscreen: bool,
    frame: WindowedFrame,
) -> LocalSettings {
    let (window_x, window_y) = match frame.position {
        Some(position) => (Some(position.x), Some(position.y)),
        None => (None, None),
    };
    LocalSettings {
        version: LOCAL_SETTINGS_VERSION,
        fullscreen,
        window_x,
        window_y,
        window_width: frame.size.x,
        window_height: frame.size.y,
        fullscreen_resolution: settings.rendering.fullscreen_resolution,
        vsync: settings.rendering.vsync,
        msaa_samples: settings.rendering.msaa_samples,
        portal_view_budget: settings.rendering.portal_view_budget,
        mouse_sensitivity: settings.input.mouse_sensitivity,
        invert_y: settings.input.invert_y,
        fov_degrees: settings.camera.fov_degrees.first_person,
        shake_scale: settings.camera.shake.scale,
        master_volume: global_volume.volume.to_linear(),
        show_diagnostics: settings.hud.show_diagnostics,
        rearview_mirror: settings.camera.rearview.enabled,
    }
}

#[cfg(test)]
mod tests {
    use bevy::audio::Volume;

    use super::*;

    fn snapshot(fullscreen: bool) -> LocalSettings {
        let settings = ClientSettings::load_default().expect("shipped client config should load");
        let frame = WindowedFrame {
            position: Some(IVec2::new(100, 80)),
            size: UVec2::new(1200, 800),
            position_pending: false,
        };
        local_settings(&settings, &GlobalVolume::new(Volume::Linear(0.5)), fullscreen, frame)
    }

    #[test]
    fn a_stable_change_saves() {
        assert!(should_save(true, &snapshot(false), &snapshot(true)));
    }

    #[test]
    fn a_change_still_settling_waits() {
        assert!(!should_save(false, &snapshot(false), &snapshot(true)));
    }

    #[test]
    fn an_unchanged_snapshot_does_not_save() {
        let current = snapshot(false);
        assert!(!should_save(true, &current, &current));
    }
}
