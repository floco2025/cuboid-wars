use super::*;
use crate::{
    cameras::{CameraViewMode, FollowCamera},
    input::input_cursor_capture_system,
};
use bevy::window::{CursorGrabMode, CursorOptions};

fn app() -> App {
    let mut app = App::new();
    app.insert_resource(ButtonInput::<KeyCode>::default())
        .insert_resource(ConsoleState::default())
        .insert_resource(SettingsMenuState::default())
        .init_resource::<CameraInputState>()
        .init_resource::<CameraViewMode>()
        .init_resource::<FollowCamera>()
        .init_resource::<ButtonInput<MouseButton>>()
        .add_systems(
            Update,
            (settings_menu_toggle_system, input_cursor_capture_system).chain(),
        );
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
