use bevy::prelude::*;
use common::protocol::{CMove, ClientMessage, FaceYaw, PlayerInput, PlayerMoveIntent, Position, ServerTick};

use crate::{
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
};

// Once per fixed tick, send the local player's input (move intent + facing)
// to the server, changed or not, so the next commit heals a lost one; nothing
// is sent while dead, since the server drops a dead player's input. The
// commit carries how many portal crossings our own simulation of the player
// has made: the intent is expressed on that side of them, and the server
// applies it only once its player is there too. Jump is event-shaped and
// sent immediately by `input_movement_system` — not handled here.
pub fn commit_player_input_system(
    to_server: Res<ClientToServerChannel>,
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    local_player_query: Query<(&PlayerMoveIntent, &FaceYaw), With<LocalPlayerMarker>>,
) {
    let Ok((move_intent, face_yaw)) = local_player_query.single() else {
        return;
    };
    if local_player_info.is_dead {
        return;
    }
    let hops = players.get(&my_player_id.0).map_or(0, |info| info.hops);
    local_player_info.move_seq = local_player_info.move_seq.wrapping_add(1);
    let _ = to_server.send(ClientToServer::Send(ClientMessage::Move(CMove {
        seq: local_player_info.move_seq,
        input: PlayerInput {
            move_intent: *move_intent,
            face_yaw: face_yaw.0,
        },
        hops,
    })));
}

// After this tick's movement, remember where the newest `CMove` left us,
// under its sequence, our crossing count, and the tick we simulated, for
// the server's echo of that `CMove` to be measured against. Runs after the
// portal transit so a crossing is in the record the way it is in the
// server's.
pub fn record_committed_position_system(
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    server_tick: Res<ServerTick>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
) {
    let Ok(pos) = local_player_query.single() else {
        return;
    };
    if local_player_info.is_dead {
        return;
    }
    let hops = players.get(&my_player_id.0).map_or(0, |info| info.hops);
    let seq = local_player_info.move_seq;
    local_player_info
        .committed_positions
        .record(seq, hops, server_tick.0, *pos);
}
