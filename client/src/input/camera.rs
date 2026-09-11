use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

use crate::{
    cameras::{CameraInputState, CameraViewMode, FollowCamera},
    config::ClientSettings,
    constants::INPUT_ZOOM_PIXELS_PER_LINE,
    players::LocalPlayerMarker,
    ui::{ConsoleState, SettingsMenuState},
};

// Cmd/Ctrl turn F into the fullscreen shortcut instead of the facing lock.
pub(super) fn fullscreen_shortcut_modifier(keyboard: &ButtonInput<KeyCode>) -> bool {
    keyboard.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ])
}

// F toggles the third-person facing lock: the body follows the view while
// locked and the mouse orbits it while unlocked.
pub fn input_facing_lock_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    view_mode: Res<CameraViewMode>,
    mut follow: ResMut<FollowCamera>,
) {
    if *view_mode == CameraViewMode::ThirdPerson
        && keyboard.just_pressed(KeyCode::KeyF)
        && !fullscreen_shortcut_modifier(&keyboard)
    {
        follow.locked = !follow.locked;
    }
}

// Wheel zoom between first and third person. Like the movement input it
// waits for the local player to exist, and an open overlay swallows the
// wheel.
pub fn input_camera_zoom_system(
    wheel: Res<AccumulatedMouseScroll>,
    view_mode: Res<CameraViewMode>,
    state: Res<CameraInputState>,
    console: Res<ConsoleState>,
    menu: Res<SettingsMenuState>,
    client_settings: Res<ClientSettings>,
    local_players: Query<(), With<LocalPlayerMarker>>,
    mut follow: ResMut<FollowCamera>,
) {
    if local_players.is_empty() {
        return;
    }
    if state.released || console.open || menu.open {
        return;
    }
    let zoom = match wheel.unit {
        MouseScrollUnit::Line => wheel.delta.y,
        MouseScrollUnit::Pixel => wheel.delta.y / INPUT_ZOOM_PIXELS_PER_LINE,
    };
    follow.zoom(
        *view_mode,
        zoom,
        client_settings.preferences.zoom_sensitivity,
        client_settings.camera.follow,
    );
}
