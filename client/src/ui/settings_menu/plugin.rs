use bevy::audio::GlobalVolume;
use bevy::prelude::*;

use super::observers::{on_checkbox_value_change, on_cycler_activate, on_slider_value_change};
use super::spawn::settings_menu_lifecycle_system;
use super::state::{SettingsMenuState, menu_open};
use super::style::{settings_menu_slider_sync_system, settings_menu_style_system, settings_menu_window_sync_system};
use super::toggle::settings_menu_toggle_system;
use super::volume::apply_global_volume_system;
use crate::schedule::ClientSet;
use crate::ui::console::console_input_system;

pub fn settings_menu_plugin(app: &mut App) {
    app.init_resource::<SettingsMenuState>();
    app.add_observer(on_slider_value_change);
    app.add_observer(on_checkbox_value_change);
    app.add_observer(on_cycler_activate);
    // After the console's keystroke system, so its Esc handling wins.
    app.add_systems(
        Update,
        settings_menu_toggle_system
            .in_set(ClientSet::Console)
            .after(console_input_system),
    );
    app.add_systems(
        Update,
        (
            settings_menu_lifecycle_system.run_if(resource_changed::<SettingsMenuState>),
            (
                settings_menu_style_system,
                settings_menu_slider_sync_system,
                settings_menu_window_sync_system,
            )
                .run_if(menu_open)
                .after(settings_menu_lifecycle_system),
        )
            .in_set(ClientSet::Hud),
    );
    app.add_systems(
        Update,
        // Before `ClientSet::Sky`, so rain's own per-frame volume write wins.
        apply_global_volume_system
            .run_if(resource_changed::<GlobalVolume>)
            .before(ClientSet::Sky),
    );
}
