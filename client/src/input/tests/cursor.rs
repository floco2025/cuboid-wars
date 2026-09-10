use super::*;
use crate::cameras::{CameraViewMode, FollowCamera};
fn app(view: CameraViewMode) -> App {
    let mut app = App::new();
    app.insert_resource(view)
        .init_resource::<FollowCamera>()
        .init_resource::<CameraInputState>()
        .init_resource::<ConsoleState>()
        .init_resource::<SettingsMenuState>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_systems(Update, input_cursor_capture_system);
    app.world_mut().spawn(CursorOptions::default());
    app
}
#[test]
fn unlocked_orbit_keeps_mouse_captured_and_menu_restores_it() {
    let mut app = app(CameraViewMode::ThirdPerson);
    app.world_mut().resource_mut::<FollowCamera>().locked = false;
    app.update();
    let cursor = app
        .world_mut()
        .query::<&CursorOptions>()
        .single(app.world())
        .expect("cursor options missing from test window");
    assert_eq!(cursor.grab_mode, CursorGrabMode::Locked);
    app.world_mut().resource_mut::<SettingsMenuState>().open = true;
    app.update();
    let cursor = app
        .world_mut()
        .query::<&CursorOptions>()
        .single(app.world())
        .expect("cursor options missing from test window");
    assert_eq!(cursor.grab_mode, CursorGrabMode::None);
    app.world_mut().resource_mut::<SettingsMenuState>().open = false;
    app.update();
    let cursor = app
        .world_mut()
        .query::<&CursorOptions>()
        .single(app.world())
        .expect("cursor options missing from test window");
    assert_eq!(cursor.grab_mode, CursorGrabMode::Locked);
}
#[test]
fn recapture_click_does_not_fire() {
    let mut app = app(CameraViewMode::FirstPerson);
    app.world_mut().resource_mut::<CameraInputState>().released = true;
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    let input = app.world().resource::<CameraInputState>();
    assert!(!input.released);
    assert!(input.suppress_fire);
}
