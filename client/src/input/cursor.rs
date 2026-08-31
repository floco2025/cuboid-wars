use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

pub fn input_cursor_capture_system(
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor_options: Single<&mut CursorOptions>,
) {
    if cursor_options.grab_mode != CursorGrabMode::None || mouse.get_just_pressed().next().is_none() {
        return;
    }
    cursor_options.visible = false;
    cursor_options.grab_mode = CursorGrabMode::Locked;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_captures_released_cursor() {
        let mut app = App::new();
        app.insert_resource(ButtonInput::<MouseButton>::default())
            .add_systems(Update, input_cursor_capture_system);
        app.world_mut().spawn(CursorOptions {
            visible: true,
            grab_mode: CursorGrabMode::None,
            ..default()
        });
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);

        app.update();

        let cursor = app
            .world_mut()
            .query::<&CursorOptions>()
            .single(app.world())
            .expect("one cursor options component missing from test app");
        assert!(!cursor.visible);
        assert_eq!(cursor.grab_mode, CursorGrabMode::Locked);
    }
}
