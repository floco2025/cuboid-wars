use bevy::{
    picking::hover::Hovered,
    prelude::*,
    ui::{Checked, InteractionDisabled, Pressed},
    ui_widgets::{SliderRange, SliderValue},
    window::{Monitor, OnMonitor, PrimaryWindow, WindowMode},
};

use super::state::{
    CheckboxSetting, CyclerButton, CyclerSetting, CyclerValueLabel, MenuCheckBoxMarker, MenuCheckMarkMarker,
    MenuSliderThumbMarker, SliderSetting, SliderValueLabel,
};
use crate::{
    config::ClientSettings,
    constants::{SETTINGS_ACCENT_COLOR, SETTINGS_OUTLINE_COLOR, SETTINGS_SLIDER_TRACK_COLOR},
};

// Nothing change-detects these colors, so the restyle writes unconditionally while the menu is open.
pub(super) fn settings_menu_style_system(
    mut cycler_buttons: Query<
        (&Hovered, Has<Pressed>, Has<InteractionDisabled>, &mut BackgroundColor),
        With<CyclerButton>,
    >,
    checkboxes: Query<(Entity, &Hovered, Has<Checked>), With<CheckboxSetting>>,
    sliders: Query<(Entity, &Hovered), With<SliderSetting>>,
    children: Query<&Children>,
    mut check_boxes: Query<&mut BorderColor, With<MenuCheckBoxMarker>>,
    mut check_marks: Query<&mut BackgroundColor, (With<MenuCheckMarkMarker>, Without<CyclerButton>)>,
    mut thumbs: Query<
        &mut BackgroundColor,
        (
            With<MenuSliderThumbMarker>,
            Without<CyclerButton>,
            Without<MenuCheckMarkMarker>,
        ),
    >,
) {
    for (hovered, pressed, disabled, mut color) in &mut cycler_buttons {
        let target = if disabled {
            SETTINGS_SLIDER_TRACK_COLOR.with_alpha(0.2)
        } else if pressed {
            SETTINGS_ACCENT_COLOR
        } else if hovered.get() {
            SETTINGS_SLIDER_TRACK_COLOR.lighter(0.15)
        } else {
            SETTINGS_SLIDER_TRACK_COLOR
        };
        color.0 = target;
    }

    for (entity, hovered, checked) in &checkboxes {
        for child in children.iter_descendants(entity) {
            if let Ok(mut border) = check_boxes.get_mut(child) {
                let target = if hovered.get() {
                    SETTINGS_OUTLINE_COLOR.lighter(0.25)
                } else {
                    SETTINGS_OUTLINE_COLOR
                };
                border.set_all(target);
            }
            if let Ok(mut mark) = check_marks.get_mut(child) {
                let target = if checked { SETTINGS_ACCENT_COLOR } else { Color::NONE };
                mark.0 = target;
            }
        }
    }

    for (entity, hovered) in &sliders {
        for child in children.iter_descendants(entity) {
            if let Ok(mut thumb) = thumbs.get_mut(child) {
                let target = if hovered.get() {
                    SETTINGS_ACCENT_COLOR.lighter(0.15)
                } else {
                    SETTINGS_ACCENT_COLOR
                };
                thumb.0 = target;
            }
        }
    }
}

pub(super) fn settings_menu_slider_sync_system(
    sliders: Query<(Entity, &SliderValue, &SliderRange, &SliderSetting), Changed<SliderValue>>,
    children: Query<&Children>,
    mut thumbs: Query<&mut Node, With<MenuSliderThumbMarker>>,
    mut labels: Query<(&SliderValueLabel, &mut Text)>,
) {
    // Equal node or text writes would make Bevy recompute the menu UI tree.
    for (entity, value, range, _) in &sliders {
        for child in children.iter_descendants(entity) {
            if let Ok(mut node) = thumbs.get_mut(child) {
                let left = Val::Percent(range.thumb_position(value.0) * 100.0);
                if node.left != left {
                    node.left = left;
                }
            }
        }
    }
    for (label, mut text) in &mut labels {
        let Some((_, value, _, _)) = sliders.iter().find(|(_, _, _, setting)| **setting == label.0) else {
            continue;
        };
        let rendered = slider_label(label.0, value.0);
        text.set_if_neq(Text(rendered));
    }
}

fn slider_label(setting: SliderSetting, value: f32) -> String {
    match setting {
        SliderSetting::MouseSensitivity => format!("{:.1}", value * 1000.0),
        SliderSetting::Fov => format!("{value:.0}"),
        SliderSetting::ShakeScale => format!("{value:.1}x"),
        SliderSetting::MasterVolume => format!("{:.0}%", value * 100.0),
    }
}

// Cycler readouts come from the live sources every frame, so nothing the
// user changes elsewhere (drag-resize, Cmd+F) can leave them stale. The
// resolution row is a fullscreen setting and goes inactive while windowed.
// Equal `Text` writes would rerun text layout every frame.
pub(super) fn settings_menu_window_sync_system(
    windows: Query<(&Window, Option<&OnMonitor>), With<PrimaryWindow>>,
    monitors: Query<&Monitor>,
    settings: Res<ClientSettings>,
    mut labels: Query<(&CyclerValueLabel, &mut Text, &mut TextColor)>,
    buttons: Query<(Entity, &CyclerButton, Has<InteractionDisabled>)>,
    mut commands: Commands,
) {
    let Ok((window, on_monitor)) = windows.single() else {
        return;
    };
    let monitor = on_monitor.and_then(|on_monitor| monitors.get(on_monitor.0).ok());
    let windowed = matches!(window.mode, WindowMode::Windowed);
    let deferred = settings.rendering.opaque_renderer.is_deferred();
    for (label, mut text, mut color) in &mut labels {
        let (rendered, dimmed) = match label.0 {
            CyclerSetting::Resolution => {
                // Effective height: the renderer never exceeds the monitor.
                let height = monitor.map_or(settings.rendering.fullscreen_resolution, |monitor| {
                    settings.rendering.fullscreen_resolution.min(monitor.physical_height)
                });
                // Width follows the monitor aspect, matching `scene_image_size`.
                let width = monitor.map_or(height * 16 / 9, |monitor| {
                    (monitor.physical_width as f32 * height as f32 / monitor.physical_height as f32).round() as u32
                });
                (format!("{width}x{height}"), windowed)
            }
            CyclerSetting::Msaa => {
                let samples = settings.rendering.msaa_samples;
                let label = if samples <= 1 {
                    "Off".to_owned()
                } else {
                    format!("{samples}x")
                };
                (label, deferred)
            }
            CyclerSetting::PortalViews => {
                let budget = settings.rendering.portal_view_budget;
                let label = if budget == 0 {
                    "Off".to_owned()
                } else {
                    budget.to_string()
                };
                (label, false)
            }
            CyclerSetting::WindowMode => (if windowed { "Windowed" } else { "Fullscreen" }.to_owned(), false),
        };
        text.set_if_neq(Text(rendered));
        let target = if dimmed {
            Color::WHITE.with_alpha(0.35)
        } else {
            Color::WHITE
        };
        color.0 = target;
    }
    for (entity, button, disabled) in &buttons {
        let desired = match button.setting {
            CyclerSetting::Resolution => windowed,
            CyclerSetting::Msaa => deferred,
            CyclerSetting::PortalViews => false,
            CyclerSetting::WindowMode => false,
        };
        if desired != disabled {
            if desired {
                commands.entity(entity).insert(InteractionDisabled);
            } else {
                commands.entity(entity).remove::<InteractionDisabled>();
            }
        }
    }
}
