use bevy::audio::Volume;

use super::*;

fn snapshot(fullscreen: bool) -> LocalSettings {
    let settings = ClientSettings::load_default().expect("shipped client config rejected");
    let frame = WindowedFrame {
        position: Some(IVec2::new(100, 80)),
        size: UVec2::new(1200, 800),
        position_pending: false,
        focus_pending: false,
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
                focus_pending: false,
            },
        );
        assert_eq!(local.preferences.mouse_sensitivity, coordinate.exp2());
        assert_eq!(local.preferences.zoom_sensitivity, coordinate.exp2());
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
