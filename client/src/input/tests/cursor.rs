use super::*;
use crate::cameras::{CameraViewMode, FollowCamera};
use bevy::ecs::change_detection::Tick;
fn app(view: CameraViewMode) -> App {
    let mut app = App::new();
    app.insert_resource(view)
        .init_resource::<FollowCamera>()
        .init_resource::<CameraInputState>()
        .init_resource::<ConsoleState>()
        .init_resource::<SettingsMenuState>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_message::<WindowFocused>()
        .add_systems(Update, input_cursor_capture_system);
    app.world_mut()
        .spawn((Window::default(), PrimaryWindow, CursorOptions::default()));
    app
}
fn primary_window(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world())
        .expect("primary window missing from test app")
}
fn cursor_changed_tick(app: &mut App) -> Tick {
    app.world_mut()
        .query::<Ref<CursorOptions>>()
        .single(app.world())
        .expect("cursor options missing from test window")
        .last_changed()
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
#[test]
fn focus_gain_centres_the_pointer_and_grabs_again() {
    let mut app = app(CameraViewMode::FirstPerson);
    app.update();
    app.update();
    let settled = cursor_changed_tick(&mut app);
    let window = primary_window(&mut app);
    app.world_mut().write_message(WindowFocused { window, focused: false });
    app.update();
    assert_eq!(cursor_changed_tick(&mut app), settled);
    app.world_mut().write_message(WindowFocused { window, focused: true });
    app.update();
    assert_ne!(cursor_changed_tick(&mut app), settled);
    let window = app.world().get::<Window>(window).expect("window missing from test app");
    assert_eq!(
        window.physical_cursor_position(),
        Some(window.physical_size().as_vec2() / 2.0)
    );
}
#[test]
fn focus_gain_under_an_overlay_leaves_the_pointer_alone() {
    let mut app = app(CameraViewMode::FirstPerson);
    app.world_mut().resource_mut::<SettingsMenuState>().open = true;
    let window = primary_window(&mut app);
    app.world_mut().write_message(WindowFocused { window, focused: true });
    app.update();
    let (window, cursor) = app
        .world_mut()
        .query::<(&Window, &CursorOptions)>()
        .single(app.world())
        .expect("window missing from test app");
    assert_eq!(window.physical_cursor_position(), None);
    assert_eq!(cursor.grab_mode, CursorGrabMode::None);
}
