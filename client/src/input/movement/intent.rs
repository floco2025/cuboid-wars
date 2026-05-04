use bevy::{math::Vec2, prelude::*};
use common::protocol::PlayerMoveIntent;

use crate::players::{MyPlayerId, PlayerMap};

pub(super) fn calculate_move_intent(
    keyboard: &Res<ButtonInput<KeyCode>>,
    face_yaw: f32,
    stunned: bool,
) -> PlayerMoveIntent {
    if stunned {
        return PlayerMoveIntent::Idle;
    }

    let mut keyboard_vec = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        keyboard_vec.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        keyboard_vec.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        keyboard_vec.x += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        keyboard_vec.x -= 1.0;
    }

    if keyboard_vec.length_squared() > 0.0 {
        let normalized_input = keyboard_vec.normalize();
        let angle_offset = normalized_input.x.atan2(normalized_input.y);
        let direction = face_yaw + angle_offset;
        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
            PlayerMoveIntent::Running { direction }
        } else {
            PlayerMoveIntent::Walking { direction }
        }
    } else {
        PlayerMoveIntent::Idle
    }
}

pub(super) fn local_player_stunned(my_player_id: Option<&Res<MyPlayerId>>, players: &Res<PlayerMap>) -> bool {
    my_player_id
        .and_then(|my_id| players.0.get(&my_id.0))
        .is_some_and(|player_info| player_info.stunned)
}
