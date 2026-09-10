use bevy::{
    prelude::*,
    window::{PrimaryWindow, WindowFocused},
};
use common::protocol::PlayerMoveIntent;

use crate::players::LocalPlayerMarker;

pub(super) fn input_focus_system(
    mut focus: MessageReader<WindowFocused>,
    windows: Query<Entity, With<PrimaryWindow>>,
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut players: Query<&mut PlayerMoveIntent, With<LocalPlayerMarker>>,
) {
    // Bevy can keep keys held when focus is lost and regained in the same frame.
    let mut lost_focus = false;
    for event in focus.read() {
        lost_focus |= !event.focused && windows.contains(event.window);
    }
    if !lost_focus {
        return;
    }
    keyboard.reset_all();
    mouse.reset_all();
    for mut intent in &mut players {
        *intent = PlayerMoveIntent::Idle;
    }
}
