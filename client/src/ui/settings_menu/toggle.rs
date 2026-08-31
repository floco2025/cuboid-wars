use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

use super::state::SettingsMenuState;
use crate::ui::ConsoleState;

// Shift+Esc releases the cursor without opening the overlay. Plain Esc toggles
// the overlay and with it the cursor. The console has Esc priority: its input
// system runs just before this one, so an Esc that closed the console leaves
// `ConsoleState` marked changed and is swallowed.
pub(super) fn settings_menu_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    console: Res<ConsoleState>,
    mut menu: ResMut<SettingsMenuState>,
    mut cursor_options: Single<&mut CursorOptions>,
) {
    if !keyboard.just_pressed(KeyCode::Escape) {
        return;
    }
    if console.open || console.is_changed() {
        return;
    }
    let shift_pressed = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    if shift_pressed {
        cursor_options.visible = true;
        cursor_options.grab_mode = CursorGrabMode::None;
        return;
    }
    menu.open = !menu.open;
    cursor_options.visible = menu.open;
    cursor_options.grab_mode = if menu.open {
        CursorGrabMode::None
    } else {
        CursorGrabMode::Locked
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ConsoleState::default())
            .insert_resource(SettingsMenuState::default())
            .add_systems(Update, settings_menu_toggle_system);
        app.world_mut().spawn(CursorOptions {
            visible: false,
            grab_mode: CursorGrabMode::Locked,
            ..default()
        });
        app.update();
        app
    }

    #[test]
    fn shift_escape_releases_cursor_without_opening_menu() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);

        app.update();

        assert!(!app.world().resource::<SettingsMenuState>().open);
        let cursor = app
            .world_mut()
            .query::<&CursorOptions>()
            .single(app.world())
            .expect("one cursor options component missing from test app");
        assert!(cursor.visible);
        assert_eq!(cursor.grab_mode, CursorGrabMode::None);
    }
}
