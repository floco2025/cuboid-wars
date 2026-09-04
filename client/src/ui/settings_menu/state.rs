use bevy::prelude::*;

// The settings overlay. While open the cursor is free and gameplay input
// stands down (`menu_closed` gates, mirroring the console's `console_closed`).
#[derive(Resource, Default)]
pub struct SettingsMenuState {
    pub open: bool,
}

pub fn menu_closed(menu: Res<SettingsMenuState>) -> bool {
    !menu.open
}

pub(super) fn menu_open(menu: Res<SettingsMenuState>) -> bool {
    menu.open
}

// Which setting a widget edits; the global observers key their apply path
// off these instead of per-entity closures.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) enum SliderSetting {
    MouseSensitivity,
    Fov,
    ShakeScale,
    MasterVolume,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) enum CheckboxSetting {
    VSync,
    InvertY,
    RearviewMirror,
    ShowDiagnostics,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) enum CyclerSetting {
    Resolution,
    Msaa,
    PortalViews,
    WindowMode,
}

#[derive(Component, Clone, Copy)]
pub(super) struct CyclerButton {
    pub setting: CyclerSetting,
    pub direction: i8,
}

#[derive(Component)]
pub(super) struct SettingsMenuRootMarker;

#[derive(Component)]
pub(super) struct MenuSliderThumbMarker;

#[derive(Component)]
pub(super) struct MenuCheckBoxMarker;

#[derive(Component)]
pub(super) struct MenuCheckMarkMarker;

// A slider's value readout, rendered by the sync system from the setting.
#[derive(Component)]
pub(super) struct SliderValueLabel(pub SliderSetting);

#[derive(Component)]
pub(super) struct CyclerValueLabel(pub CyclerSetting);
