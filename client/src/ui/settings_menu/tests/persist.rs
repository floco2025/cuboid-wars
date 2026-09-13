use crate::test_fixtures;
use bevy::{
    audio::Volume,
    ui_widgets::{SliderRange, SliderValue, ValueChange},
};

use super::super::{
    observers::on_slider_value_change,
    spawn::settings_menu_lifecycle_system,
    state::{SliderSetting, SliderValueLabel},
    style::settings_menu_slider_sync_system,
};
use super::*;

fn snapshot(fullscreen: bool) -> LocalSettings {
    let settings = test_fixtures::client_settings();
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
    let mut app = App::new();
    let settings = test_fixtures::client_settings();
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
        let mut restored = test_fixtures::client_settings();
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

fn audio_slider(app: &mut App, setting: SliderSetting) -> (Entity, f32) {
    let world = app.world_mut();
    let (entity, _, value, range) = world
        .query::<(Entity, &SliderSetting, &SliderValue, &SliderRange)>()
        .iter(world)
        .find(|(_, candidate, _, _)| **candidate == setting)
        .expect("audio slider missing");
    assert_eq!(range.thumb_position(-20.0), 0.0);
    assert_eq!(range.thumb_position(0.0), 0.5);
    assert_eq!(range.thumb_position(20.0), 1.0);
    (entity, value.0)
}

#[test]
fn audio_sliders_apply_db_and_off_independently_and_restore_after_saving() {
    let mut app = App::new();
    app.insert_resource(test_fixtures::client_settings())
        .init_resource::<GlobalVolume>()
        .insert_resource(SettingsMenuState { open: true })
        .add_observer(on_slider_value_change)
        .add_systems(
            Update,
            (settings_menu_lifecycle_system, settings_menu_slider_sync_system).chain(),
        );
    app.update();
    for setting in [
        SliderSetting::MasterVolume,
        SliderSetting::FootstepVolume,
        SliderSetting::ActorMovementVolume,
    ] {
        assert_eq!(audio_slider(&mut app, setting).1, 0.0);
    }
    for (master_db, footstep_db, movement_db) in [
        (-20.0, -6.0, 3.0),
        (-6.0, -20.0, 6.0),
        (0.0, 0.0, -20.0),
        (6.0, -12.0, 0.0),
        (20.0, 20.0, 20.0),
    ] {
        for (setting, value) in [
            (SliderSetting::MasterVolume, master_db),
            (SliderSetting::FootstepVolume, footstep_db),
            (SliderSetting::ActorMovementVolume, movement_db),
        ] {
            let (source, _) = audio_slider(&mut app, setting);
            app.world_mut().trigger(ValueChange::<f32> {
                source,
                value,
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
        let encoded = serde_json::to_string(&local).expect("audio settings failed to serialize");
        let saved: LocalSettings = serde_json::from_str(&encoded).expect("audio settings failed to deserialize");
        let expected_master = if master_db == -20.0 {
            0.0
        } else {
            10.0_f32.powf(master_db / 20.0)
        };
        assert_eq!(saved.master_volume, expected_master);
        assert_eq!(saved.preferences.footstep_volume_db, footstep_db);
        assert_eq!(saved.preferences.actor_movement_volume_db, movement_db);

        app.world_mut().resource_mut::<SettingsMenuState>().open = false;
        app.update();
        saved.apply_to(&mut app.world_mut().resource_mut::<ClientSettings>());
        app.world_mut().resource_mut::<GlobalVolume>().volume = Volume::Linear(saved.master_volume);
        app.world_mut().resource_mut::<SettingsMenuState>().open = true;
        app.update();
        for (setting, expected) in [
            (SliderSetting::MasterVolume, master_db),
            (SliderSetting::FootstepVolume, footstep_db),
            (SliderSetting::ActorMovementVolume, movement_db),
        ] {
            assert!((audio_slider(&mut app, setting).1 - expected).abs() < 0.00001);
            let world = app.world_mut();
            let (_, label) = world
                .query::<(&SliderValueLabel, &Text)>()
                .iter(world)
                .find(|(label, _)| label.0 == setting)
                .expect("audio readout missing");
            assert_eq!(
                label.0,
                if expected == -20.0 {
                    "Off".to_owned()
                } else {
                    format!("{expected:.0} dB")
                }
            );
        }
    }
}
