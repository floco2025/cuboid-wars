use bevy::prelude::*;
use common::{math::angle_delta_radians, protocol::*};

use crate::{
    constants::{FACE_CHANGE_THRESHOLD, FACE_SEND_COOLDOWN, MOVE_INPUT_DIR_CHANGE_THRESHOLD, MOVE_INPUT_SEND_COOLDOWN},
    network::{ClientToServer, ClientToServerChannel},
    players::LocalPlayerInfo,
};

pub(super) fn send_throttled_updates(
    move_intent: PlayerMoveIntent,
    face_yaw: f32,
    jump_requested: bool,
    time: &Res<Time>,
    to_server: &Res<ClientToServerChannel>,
    local_player_info: &mut LocalPlayerInfo,
) {
    let delta = time.delta_secs();
    local_player_info.last_send_input_time += delta;
    local_player_info.last_send_face_time += delta;

    let last_dir = local_player_info.last_sent_move_intent.direction();
    let new_dir = move_intent.direction();
    let active_changed = last_dir.is_some() != new_dir.is_some();
    let mode_changed = move_intent.is_running() != local_player_info.last_sent_move_intent.is_running();
    let direction_changed = match (new_dir, last_dir) {
        (Some(new_d), Some(old_d)) => {
            angle_delta_radians(new_d, old_d).abs() > MOVE_INPUT_DIR_CHANGE_THRESHOLD.to_radians()
        }
        _ => false,
    };
    if active_changed
        || mode_changed
        || (direction_changed && local_player_info.last_send_input_time >= MOVE_INPUT_SEND_COOLDOWN)
    {
        let msg = ClientMessage::PlayerMoveIntent(CPlayerMoveIntent { move_intent });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_move_intent = move_intent;
        local_player_info.last_send_input_time = 0.0;
    }

    let face_changed =
        angle_delta_radians(face_yaw, local_player_info.last_sent_face).abs() > FACE_CHANGE_THRESHOLD.to_radians();
    if face_changed && local_player_info.last_send_face_time >= FACE_SEND_COOLDOWN {
        let msg = ClientMessage::Face(CFace { dir: face_yaw });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_face = face_yaw;
        local_player_info.last_send_face_time = 0.0;
    }

    if jump_requested {
        let msg = ClientMessage::Jump(CJump {});
        let _ = to_server.send(ClientToServer::Send(msg));
    }
}
