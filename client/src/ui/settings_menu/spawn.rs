use bevy::audio::GlobalVolume;
use bevy::prelude::*;
use bevy::ui::Checked;
use bevy::window::{PresentMode, PrimaryWindow};

use super::state::{CheckboxSetting, CyclerSetting, SettingsMenuRoot, SettingsMenuState, SliderSetting};
use super::widgets::{checkbox_row, cycler_row, section_header, slider_row};
use crate::config::ClientSettings;
use crate::constants::{HUD_ROW_GAP_PX, SETTINGS_BACKDROP_COLOR, SETTINGS_OUTLINE_COLOR, SETTINGS_PANEL_BG_COLOR};

// Spawned on open and despawned on close, so every open reads the live
// values and the widgets never go stale.
pub(super) fn settings_menu_lifecycle_system(
    menu: Res<SettingsMenuState>,
    existing: Query<Entity, With<SettingsMenuRoot>>,
    settings: Res<ClientSettings>,
    global_volume: Res<GlobalVolume>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    if !menu.open {
        for entity in &existing {
            commands.entity(entity).despawn();
        }
        return;
    }
    if !existing.is_empty() {
        return;
    }

    let font = settings.hud.font_sizes.settings_menu;
    let dims = settings.hud.settings_menu;
    let vsync = windows
        .single()
        .is_ok_and(|window| window.present_mode == PresentMode::Fifo);

    commands
        .spawn((
            SettingsMenuRoot,
            GlobalZIndex(10),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(SETTINGS_BACKDROP_COLOR),
        ))
        .with_children(|backdrop| {
            backdrop
                .spawn((
                    Node {
                        width: Val::Px(dims.panel_width),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(12.0)),
                        row_gap: Val::Px(HUD_ROW_GAP_PX),
                        ..default()
                    },
                    BackgroundColor(SETTINGS_PANEL_BG_COLOR),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Settings"),
                        TextFont {
                            font_size: FontSize::Px(font),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        Node {
                            align_self: AlignSelf::Center,
                            ..default()
                        },
                    ));

                    panel.spawn(section_header("Display", font));
                    panel.spawn(cycler_row(
                        "Window mode",
                        font,
                        dims.control_width,
                        CyclerSetting::WindowMode,
                    ));
                    panel.spawn(cycler_row(
                        "Fullscreen resolution",
                        font,
                        dims.control_width,
                        CyclerSetting::Resolution,
                    ));
                    let mut vsync_row = panel.spawn(checkbox_row("VSync", font, CheckboxSetting::VSync));
                    if vsync {
                        vsync_row.insert(Checked);
                    }

                    panel.spawn(section_header("Graphics", font));
                    panel.spawn(cycler_row(
                        "Anti-aliasing",
                        font,
                        dims.control_width,
                        CyclerSetting::Msaa,
                    ));
                    panel.spawn(cycler_row(
                        "Portal recursion",
                        font,
                        dims.control_width,
                        CyclerSetting::PortalRecursion,
                    ));
                    panel.spawn(section_header("Controls", font));
                    panel.spawn(slider_row(
                        "Mouse sensitivity",
                        font,
                        dims.control_width,
                        SliderSetting::MouseSensitivity,
                        0.0005,
                        0.006,
                        settings.input.mouse_sensitivity,
                        4,
                    ));
                    let mut invert_row = panel.spawn(checkbox_row("Invert Y", font, CheckboxSetting::InvertY));
                    if settings.input.invert_y {
                        invert_row.insert(Checked);
                    }

                    panel.spawn(section_header("Camera", font));
                    panel.spawn(slider_row(
                        "Field of view",
                        font,
                        dims.control_width,
                        SliderSetting::Fov,
                        60.0,
                        110.0,
                        settings.camera.fov_degrees.first_person,
                        0,
                    ));
                    panel.spawn(slider_row(
                        "Camera shake",
                        font,
                        dims.control_width,
                        SliderSetting::ShakeScale,
                        0.0,
                        2.0,
                        settings.camera.shake.scale,
                        1,
                    ));

                    panel.spawn(section_header("Audio", font));
                    panel.spawn(slider_row(
                        "Master volume",
                        font,
                        dims.control_width,
                        SliderSetting::MasterVolume,
                        0.0,
                        2.0,
                        global_volume.volume.to_linear(),
                        2,
                    ));

                    panel.spawn(section_header("HUD", font));
                    let mut rearview_row =
                        panel.spawn(checkbox_row("Rearview mirror", font, CheckboxSetting::RearviewMirror));
                    if settings.camera.rearview.enabled {
                        rearview_row.insert(Checked);
                    }
                    let mut diagnostics_row = panel.spawn(checkbox_row(
                        "FPS / RTT readout",
                        font,
                        CheckboxSetting::ShowDiagnostics,
                    ));
                    if settings.hud.show_diagnostics {
                        diagnostics_row.insert(Checked);
                    }

                    panel.spawn((
                        Text::new("Esc to resume"),
                        TextFont {
                            font_size: FontSize::Px(font),
                            ..default()
                        },
                        TextColor(SETTINGS_OUTLINE_COLOR),
                        Node {
                            align_self: AlignSelf::Center,
                            margin: UiRect::top(Val::Px(8.0)),
                            ..default()
                        },
                    ));
                });
        });
}
