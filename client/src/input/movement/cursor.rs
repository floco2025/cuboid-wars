use bevy::{input::mouse::MouseMotion, prelude::*};
use common::protocol::{CPlayerMoveIntent, ClientMessage, PlayerMoveIntent};

use super::types::LocalPlayerInputQuery;
use crate::{
    network::{ClientToServer, ClientToServerChannel},
    players::LocalPlayerInfo,
};

pub(super) fn handle_unlocked_cursor(
    mouse_motion: &mut MessageReader<MouseMotion>,
    to_server: &Res<ClientToServerChannel>,
    local_player_info: &mut LocalPlayerInfo,
    local_player_query: &mut LocalPlayerInputQuery,
) {
    for _ in mouse_motion.read() {}

    if local_player_info.last_sent_move_intent.direction().is_some() {
        let idle = PlayerMoveIntent::Idle;
        for (_, mut input, _, _) in local_player_query.iter_mut() {
            *input = idle;
        }
        let msg = ClientMessage::PlayerMoveIntent(CPlayerMoveIntent { move_intent: idle });
        let _ = to_server.send(ClientToServer::Send(msg));
        local_player_info.last_sent_move_intent = idle;
        local_player_info.last_send_input_time = 0.0;
    }
}
