use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Button, Checkbox, Slider, SliderPrecision, SliderRange, SliderThumb, SliderValue, TrackClick};

use super::state::{
    CheckboxSetting, CyclerButton, CyclerSetting, CyclerValueLabel, MenuCheckBoxMarker, MenuCheckMarkMarker,
    MenuSliderThumbMarker, SliderSetting, SliderValueLabel,
};
use crate::constants::{
    CONSOLE_TEXT_COLOR, SETTINGS_ACCENT_COLOR, SETTINGS_OUTLINE_COLOR, SETTINGS_SLIDER_TRACK_COLOR,
};

const SLIDER_THUMB_PX: f32 = 12.0;
const CYCLER_BUTTON_PX: f32 = 22.0;

fn label_text(text: &str, font_size: f32) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(font_size),
            ..default()
        },
        TextColor(Color::WHITE),
    )
}

fn row() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(8.0),
        ..default()
    }
}

pub(super) fn section_header(text: &str, font_size: f32) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(font_size),
            ..default()
        },
        TextColor(CONSOLE_TEXT_COLOR),
        Node {
            margin: UiRect::top(Val::Px(8.0)),
            ..default()
        },
    )
}

// The value readout lives inside the control's fixed width, so the whole
// group matches the cyclers' footprint and the right column stays one width.
const VALUE_BOX_PX: f32 = 48.0;
const CONTROL_GAP_PX: f32 = 8.0;

#[expect(clippy::too_many_arguments, reason = "one call site per setting row")]
pub(super) fn slider_row(
    label: &str,
    font_size: f32,
    control_width: f32,
    setting: SliderSetting,
    min: f32,
    max: f32,
    value: f32,
    precision: i32,
) -> impl Bundle {
    (
        row(),
        children![
            label_text(label, font_size),
            (
                Node {
                    width: Val::Px(control_width),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(CONTROL_GAP_PX),
                    ..default()
                },
                children![
                    slider(
                        control_width - VALUE_BOX_PX - CONTROL_GAP_PX,
                        setting,
                        min,
                        max,
                        value,
                        precision,
                    ),
                    (
                        Node {
                            width: Val::Px(VALUE_BOX_PX),
                            flex_shrink: 0.0,
                            justify_content: JustifyContent::FlexEnd,
                            ..default()
                        },
                        children![(SliderValueLabel(setting), label_text("--", font_size))],
                    ),
                ],
            ),
        ],
    )
}

fn slider(control_width: f32, setting: SliderSetting, min: f32, max: f32, value: f32, precision: i32) -> impl Bundle {
    (
        Node {
            width: Val::Px(control_width),
            height: Val::Px(SLIDER_THUMB_PX),
            // shrink 0 everywhere on the control side: the label's length
            // must never squeeze a control, so all sliders stay equal.
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Stretch,
            ..default()
        },
        Hovered::default(),
        setting,
        Slider {
            track_click: TrackClick::Snap,
            ..default()
        },
        // Clamp: an out-of-range config/CLI seed would draw the thumb far
        // outside the track and be silently rewritten on first touch.
        SliderValue(value.clamp(min, max)),
        SliderRange::new(min, max),
        SliderPrecision(precision),
        Children::spawn((
            Spawn((
                Node {
                    height: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(SETTINGS_SLIDER_TRACK_COLOR),
            )),
            // Track short by one thumb width, so the thumb can be placed
            // with plain percentages (the standard-widgets example's trick).
            Spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(SLIDER_THUMB_PX),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                children![(
                    MenuSliderThumbMarker,
                    SliderThumb,
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(SLIDER_THUMB_PX),
                        height: Val::Px(SLIDER_THUMB_PX),
                        left: Val::Percent(0.0),
                        ..default()
                    },
                    BackgroundColor(SETTINGS_ACCENT_COLOR),
                )],
            )),
        )),
    )
}

// The whole row is the clickable checkbox, so the caller can `insert(Checked)`
// on the spawned row when the setting starts true.
pub(super) fn checkbox_row(label: &str, font_size: f32, setting: CheckboxSetting) -> impl Bundle {
    (
        row(),
        Hovered::default(),
        setting,
        Checkbox,
        children![
            label_text(label, font_size),
            (
                MenuCheckBoxMarker,
                Node {
                    width: Val::Px(16.0),
                    height: Val::Px(16.0),
                    flex_shrink: 0.0,
                    border: UiRect::all(Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor::all(SETTINGS_OUTLINE_COLOR),
                children![(
                    MenuCheckMarkMarker,
                    Node {
                        width: Val::Px(8.0),
                        height: Val::Px(8.0),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                )],
            ),
        ],
    )
}

pub(super) fn cycler_row(label: &str, font_size: f32, control_width: f32, setting: CyclerSetting) -> impl Bundle {
    (
        row(),
        children![
            label_text(label, font_size),
            (
                Node {
                    width: Val::Px(control_width),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                },
                children![
                    cycler_button(setting, -1, "<", font_size),
                    (
                        CyclerValueLabel(setting),
                        Text::new("--"),
                        TextFont {
                            font_size: FontSize::Px(font_size),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ),
                    cycler_button(setting, 1, ">", font_size),
                ],
            ),
        ],
    )
}

fn cycler_button(setting: CyclerSetting, direction: i8, glyph: &str, font_size: f32) -> impl Bundle {
    (
        Node {
            width: Val::Px(CYCLER_BUTTON_PX),
            height: Val::Px(CYCLER_BUTTON_PX),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Button,
        Hovered::default(),
        CyclerButton { setting, direction },
        BackgroundColor(SETTINGS_SLIDER_TRACK_COLOR),
        children![label_text(glyph, font_size)],
    )
}
