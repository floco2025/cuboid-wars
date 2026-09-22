use super::*;
use crate::{
    cameras::CameraInputState, input::input_cursor_capture_system, network::install_playback,
    schedule::configure_client_sets,
};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::MouseMotion,
    },
    window::{CursorGrabMode, CursorOptions, PrimaryWindow, WindowFocused},
};

#[test]
fn playback_keeps_escape_menu_and_cursor_controls_while_enter_does_not_open_chat() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, configure_client_sets, hud_plugin))
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ConsoleState>()
        .init_resource::<CameraInputState>()
        .add_message::<KeyboardInput>()
        .add_message::<MouseMotion>()
        .add_message::<WindowFocused>()
        .add_systems(PreUpdate, input_cursor_capture_system.in_set(ClientSet::Input));
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow, CursorOptions::default()))
        .id();
    install_playback(&mut app);
    // Use the real registered input schedule without starting the renderer.
    app.world_mut().run_schedule(PreUpdate);
    app.world_mut().clear_trackers();
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::Enter,
        logical_key: Key::Enter,
        state: ButtonState::Pressed,
        text: Some("\r".into()),
        repeat: false,
        window,
    });
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    app.world_mut().run_schedule(PreUpdate);
    assert!(!app.world().resource::<ConsoleState>().open);
    app.world_mut().clear_trackers();

    for open in [true, false] {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.world_mut().run_schedule(PreUpdate);
        assert_eq!(app.world().resource::<SettingsMenuState>().open, open);
        let cursor = app.world().get::<CursorOptions>(window).expect("cursor");
        assert_eq!(cursor.visible, open);
        assert_eq!(
            cursor.grab_mode,
            if open {
                CursorGrabMode::None
            } else {
                CursorGrabMode::Locked
            }
        );
        app.world_mut().clear_trackers();
    }
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ShiftLeft);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.world_mut().run_schedule(PreUpdate);
    assert!(!app.world().resource::<SettingsMenuState>().open);
    assert!(app.world().resource::<CameraInputState>().released);
    assert_eq!(
        app.world().get::<CursorOptions>(window).expect("cursor").grab_mode,
        CursorGrabMode::None
    );
}
