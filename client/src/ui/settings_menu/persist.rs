use std::time::Duration;

use bevy::{
    audio::GlobalVolume,
    prelude::*,
    window::{ClosingWindow, PrimaryWindow, WindowCloseRequested, WindowMode},
};

use super::state::SettingsMenuState;

use crate::{
    config::{ClientSettings, LOCAL_SETTINGS_VERSION, LocalSettings},
    input::WindowedFrame,
};

const SAVE_DELAY: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(super) struct SaveState {
    observed: Option<LocalSettings>,
    saved: Option<LocalSettings>,
    changed_at: Duration,
    menu_was_open: bool,
}

impl SaveState {
    fn pending(&mut self, current: LocalSettings, now: Duration, flush: bool) -> Option<LocalSettings> {
        if self.observed.as_ref() != Some(&current) {
            self.changed_at = now;
            self.observed = Some(current.clone());
        }
        (self.saved.as_ref() != Some(&current) && (flush || now.saturating_sub(self.changed_at) >= SAVE_DELAY))
            .then_some(current)
    }
}

pub(super) fn save_local_settings_system(
    settings: Res<ClientSettings>,
    global_volume: Res<GlobalVolume>,
    frame: Res<WindowedFrame>,
    menu: Res<SettingsMenuState>,
    time: Res<Time<Real>>,
    windows: Query<(Entity, &Window, Has<ClosingWindow>), With<PrimaryWindow>>,
    mut close_requests: MessageReader<WindowCloseRequested>,
    mut exit: MessageReader<AppExit>,
    mut state: Local<SaveState>,
) {
    let exiting = exit.read().count() > 0;
    let menu_closed = state.menu_was_open && !menu.open;
    state.menu_was_open = menu.open;
    let Ok((entity, window, closing)) = windows.single() else {
        return;
    };
    let close_requested = close_requests.read().any(|event| event.window == entity);
    let fullscreen = !matches!(window.mode, WindowMode::Windowed);
    let local = local_settings(&settings, &global_volume, fullscreen, *frame);
    let Some(local) = state.pending(
        local,
        time.elapsed(),
        exiting || closing || close_requested || menu_closed,
    ) else {
        return;
    };
    match local.save() {
        Ok(()) => state.saved = Some(local),
        Err(error) => {
            warn!("failed to save settings: {error:#}");
            state.changed_at = time.elapsed();
        }
    }
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
        master_volume: global_volume.volume.to_linear(),
        preferences: settings.preferences,
    }
}

#[cfg(test)]
#[path = "tests/persist.rs"]
mod tests;
