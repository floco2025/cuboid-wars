use bevy::audio::GlobalVolume;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowMode};

use super::state::SettingsMenuState;
use crate::config::{ClientSettings, LOCAL_SETTINGS_VERSION, LocalSettings};

// Menu edits are committed together when it closes. Changes from outside the
// panel, notably the fullscreen shortcuts, are committed immediately.
pub(super) fn save_local_settings_system(
    menu: Res<SettingsMenuState>,
    settings: Res<ClientSettings>,
    global_volume: Res<GlobalVolume>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut last_saved: Local<Option<LocalSettings>>,
    mut was_open: Local<bool>,
) {
    let fullscreen = windows
        .single()
        .is_ok_and(|window| !matches!(window.mode, WindowMode::Windowed));
    let local = local_settings(&settings, &global_volume, fullscreen);
    let menu_just_closed = *was_open && !menu.open;
    *was_open = menu.open;
    let Some(previous) = last_saved.as_ref() else {
        *last_saved = Some(local);
        return;
    };
    if !should_save(menu.open, menu_just_closed, previous, &local) {
        return;
    }
    if let Err(error) = local.save() {
        warn!("failed to save settings: {error:#}");
    }
    *last_saved = Some(local);
}

fn should_save(menu_open: bool, menu_just_closed: bool, previous: &LocalSettings, current: &LocalSettings) -> bool {
    !menu_open && (menu_just_closed || current != previous)
}

fn local_settings(settings: &ClientSettings, global_volume: &GlobalVolume, fullscreen: bool) -> LocalSettings {
    LocalSettings {
        version: LOCAL_SETTINGS_VERSION,
        fullscreen,
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
        local_settings(&settings, &GlobalVolume::new(Volume::Linear(0.5)), fullscreen)
    }

    #[test]
    fn fullscreen_change_saves_while_menu_is_closed() {
        assert!(should_save(false, false, &snapshot(false), &snapshot(true)));
    }

    #[test]
    fn changes_wait_while_menu_is_open() {
        assert!(!should_save(true, false, &snapshot(false), &snapshot(true)));
    }

    #[test]
    fn closing_menu_saves_even_when_values_are_unchanged() {
        let current = snapshot(false);
        assert!(should_save(false, true, &current, &current));
    }
}
