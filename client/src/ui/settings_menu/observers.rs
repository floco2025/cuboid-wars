use bevy::audio::{GlobalVolume, Volume};
use bevy::prelude::*;
use bevy::ui::Checked;
use bevy::ui_widgets::{Activate, SliderValue, ValueChange};
use bevy::window::{Monitor, OnMonitor, PresentMode, PrimaryMonitor, PrimaryWindow, WindowMode};

use super::state::{CheckboxSetting, CyclerButton, CyclerSetting, SliderSetting};
use crate::config::ClientSettings;
use crate::input::enter_borderless_fullscreen;

// Fullscreen render-resolution caps ("720p"): the scene renders at most
// this high and upscales to the monitor (windowed always renders native).
// The list is filtered to the monitor and topped by its native height.
const RESOLUTION_PRESETS: &[u32] = &[480, 720, 900, 1080, 1440, 2160];

pub(super) fn on_slider_value_change(
    event: On<ValueChange<f32>>,
    sliders: Query<&SliderSetting>,
    mut settings: ResMut<ClientSettings>,
    mut global_volume: ResMut<GlobalVolume>,
    mut commands: Commands,
) {
    let Ok(&setting) = sliders.get(event.source) else {
        return;
    };
    // `SliderValue` is immutable; without this re-insert the thumb freezes.
    commands.entity(event.source).insert(SliderValue(event.value));
    match setting {
        SliderSetting::MouseSensitivity => settings.input.mouse_sensitivity = event.value,
        SliderSetting::Fov => settings.camera.fov_degrees.first_person = event.value,
        SliderSetting::ShakeScale => settings.camera.shake.scale = event.value,
        SliderSetting::MasterVolume => global_volume.volume = Volume::Linear(event.value),
    }
}

pub(super) fn on_checkbox_value_change(
    event: On<ValueChange<bool>>,
    checkboxes: Query<&CheckboxSetting>,
    mut settings: ResMut<ClientSettings>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let Ok(&setting) = checkboxes.get(event.source) else {
        return;
    };
    if event.value {
        commands.entity(event.source).insert(Checked);
    } else {
        commands.entity(event.source).remove::<Checked>();
    }
    match setting {
        CheckboxSetting::VSync => {
            settings.rendering.vsync = event.value;
            if let Ok(mut window) = windows.single_mut() {
                window.present_mode = if event.value {
                    PresentMode::Fifo
                } else {
                    PresentMode::AutoNoVsync
                };
            }
        }
        CheckboxSetting::InvertY => settings.input.invert_y = event.value,
        CheckboxSetting::ShowDiagnostics => settings.hud.show_diagnostics = event.value,
    }
}

pub(super) fn on_cycler_activate(
    event: On<Activate>,
    buttons: Query<&CyclerButton>,
    mut settings: ResMut<ClientSettings>,
    mut windows: Query<(&mut Window, Option<&OnMonitor>), With<PrimaryWindow>>,
    monitors: Query<(Entity, Has<PrimaryMonitor>), With<Monitor>>,
    monitor_data: Query<&Monitor>,
) {
    let Ok(&button) = buttons.get(event.entity) else {
        return;
    };
    match button.setting {
        CyclerSetting::Resolution => {
            // Fullscreen-only setting; the row is disabled while windowed.
            let Ok((window, on_monitor)) = windows.single() else {
                return;
            };
            if matches!(window.mode, WindowMode::Windowed) {
                return;
            }
            let native = on_monitor
                .and_then(|on_monitor| monitor_data.get(on_monitor.0).ok())
                .map(|monitor| monitor.physical_height)
                .or_else(|| monitor_data.iter().map(|monitor| monitor.physical_height).max());
            let mut presets: Vec<u32> = RESOLUTION_PRESETS.to_vec();
            if let Some(native) = native {
                presets.retain(|&height| height < native);
                presets.push(native);
            }
            // Step from the preset NEAREST the effective height (the renderer
            // never exceeds the monitor), so an over-native config value
            // cannot make the first press wrap around the list.
            let current = native.map_or(settings.rendering.fullscreen_resolution, |native| {
                settings.rendering.fullscreen_resolution.min(native)
            });
            let nearest = presets
                .iter()
                .enumerate()
                .min_by_key(|(_, height)| height.abs_diff(current))
                .map_or(0, |(index, _)| index);
            let step = if button.direction < 0 { presets.len() - 1 } else { 1 };
            settings.rendering.fullscreen_resolution = presets[(nearest + step) % presets.len()];
        }
        CyclerSetting::WindowMode => {
            let Ok((mut window, on_monitor)) = windows.single_mut() else {
                return;
            };
            if matches!(window.mode, WindowMode::Windowed) {
                enter_borderless_fullscreen(&mut window, on_monitor, &monitors);
            } else {
                window.mode = WindowMode::Windowed;
            }
        }
    }
}
