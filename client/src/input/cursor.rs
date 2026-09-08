use crate::{
    cameras::CameraInputState,
    ui::{ConsoleState, SettingsMenuState},
};
use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

pub fn input_cursor_capture_system(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    console: Res<ConsoleState>,
    menu: Res<SettingsMenuState>,
    mut input: ResMut<CameraInputState>,
    mut cursor: Single<&mut CursorOptions>,
) {
    input.suppress_fire = false;
    let overlay = console.open || menu.open;
    if input.released
        && !overlay
        && !keyboard.just_pressed(KeyCode::Escape)
        && mouse.get_just_pressed().next().is_some()
    {
        input.released = false;
        input.suppress_fire = true;
    }
    let capture = !overlay && !input.released;
    let grab_mode = if capture {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    // Equal writes would send redundant cursor updates to winit.
    if cursor.visible == capture {
        cursor.visible = !capture;
    }
    if cursor.grab_mode != grab_mode {
        cursor.grab_mode = grab_mode;
    }
}

#[cfg(test)]
mod tests {
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
}
