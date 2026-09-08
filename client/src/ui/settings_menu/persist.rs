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
        fullscreen_resolution: settings.preferences.fullscreen_resolution,
        vsync: settings.preferences.vsync,
        msaa_samples: settings.preferences.msaa_samples,
        portal_view_budget: settings.preferences.portal_view_budget,
        mouse_sensitivity: settings.preferences.mouse_sensitivity,
        zoom_sensitivity: settings.preferences.zoom_sensitivity,
        invert_y: settings.preferences.invert_y,
        fov_degrees: settings.preferences.fov_degrees,
        shake_scale: settings.preferences.shake_scale,
        master_volume: global_volume.volume.to_linear(),
        show_diagnostics: settings.preferences.show_diagnostics,
        rearview_mirror: settings.preferences.rearview_mirror,
    }
}

#[cfg(test)]
mod tests {
    use bevy::audio::Volume;

    use super::*;

    fn snapshot(fullscreen: bool) -> LocalSettings {
        let settings = ClientSettings::load_default().expect("shipped client config rejected");
        let frame = WindowedFrame {
            position: Some(IVec2::new(100, 80)),
            size: UVec2::new(1200, 800),
            position_pending: false,
        };
        local_settings(&settings, &GlobalVolume::new(Volume::Linear(0.5)), fullscreen, frame)
    }

    #[test]
    fn gaps_between_drag_events_do_not_write_until_the_drag_settles() {
        let initial = snapshot(false);
        let mut state = SaveState {
            saved: Some(initial.clone()),
            ..Default::default()
        };
        for frame in 0..120 {
            let mut current = initial.clone();
            current.window_x = Some(100 + frame / 2);
            let now = Duration::from_secs_f64(f64::from(frame) / 120.0);
            assert!(state.pending(current, now, false).is_none());
        }
        let mut final_settings = initial;
        final_settings.window_x = Some(159);
        let saved = state.pending(final_settings.clone(), Duration::from_millis(1500), false);
        assert_eq!(saved, Some(final_settings.clone()));
        state.saved = saved;
        assert!(state.pending(final_settings, Duration::from_secs(2), false).is_none());
    }

    #[test]
    fn flush_saves_the_latest_change_without_waiting() {
        let mut state = SaveState {
            saved: Some(snapshot(false)),
            ..Default::default()
        };
        let current = snapshot(true);
        assert_eq!(state.pending(current.clone(), Duration::ZERO, true), Some(current));
    }

    #[test]
    fn unchanged_settings_do_not_write_on_exit() {
        let current = snapshot(false);
        let mut state = SaveState {
            saved: Some(current.clone()),
            ..Default::default()
        };
        assert!(state.pending(current, Duration::ZERO, true).is_none());
    }

    #[test]
    fn sensitivity_sliders_save_multipliers_and_restore_their_positions() {
        use super::super::{observers::on_slider_value_change, state::SliderSetting};
        use bevy::ui_widgets::{SliderValue, ValueChange};
        let mut app = App::new();
        let settings = ClientSettings::load_default().expect("client settings are invalid");
        app.insert_resource(settings).init_resource::<GlobalVolume>();
        app.add_observer(on_slider_value_change);
        let sliders = [SliderSetting::MouseSensitivity, SliderSetting::ZoomSensitivity].map(|setting| {
            app.world_mut()
                .spawn((setting, SliderValue(setting.slider_value(1.0))))
                .id()
        });
        app.update();
        for coordinate in [-2.0_f32, -1.0, 0.0, 1.0] {
            for slider in sliders {
                app.world_mut().trigger(ValueChange::<f32> {
                    source: slider,
                    value: coordinate,
                    is_final: true,
                });
            }
            app.update();
            let local = local_settings(
                app.world().resource::<ClientSettings>(),
                app.world().resource::<GlobalVolume>(),
                false,
                WindowedFrame {
                    position: None,
                    size: UVec2::new(1280, 720),
                    position_pending: false,
                },
            );
            assert_eq!(local.mouse_sensitivity, coordinate.exp2());
            assert_eq!(local.zoom_sensitivity, coordinate.exp2());
            let mut restored = ClientSettings::load_default().expect("client settings are invalid");
            local.apply_to(&mut restored);
            for (slider, setting, preference) in [
                (
                    sliders[0],
                    SliderSetting::MouseSensitivity,
                    restored.preferences.mouse_sensitivity,
                ),
                (
                    sliders[1],
                    SliderSetting::ZoomSensitivity,
                    restored.preferences.zoom_sensitivity,
                ),
            ] {
                let widget = app.world().get::<SliderValue>(slider).expect("slider value missing").0;
                assert_eq!(widget, coordinate);
                assert_eq!(setting.slider_value(preference), widget);
            }
        }
    }
}
