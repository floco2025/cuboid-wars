use bevy::prelude::*;
use common::protocol::{CMove, ClientMessage, FaceYaw, PlayerInput, PlayerMoveIntent, Position};

use crate::{
    constants::COMMIT_TELEPORT_HOLD_SECS,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
};

// Once per fixed tick, send the local player's input (move intent + facing)
// to the server, changed or not, so the next commit heals a lost one. The
// stream only holds briefly after a local portal hop, and stops while dead,
// since the server drops a dead player's input. Jump is event-shaped and sent
// immediately by `input_movement_system` — not handled here.
pub fn commit_player_input_system(
    time: Res<Time>,
    to_server: Res<ClientToServerChannel>,
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    local_player_query: Query<(&PlayerMoveIntent, &FaceYaw), With<LocalPlayerMarker>>,
) {
    let Ok((move_intent, face_yaw)) = local_player_query.single() else {
        return;
    };
    let last_teleport_time = players
        .get(&my_player_id.0)
        .map_or(f32::NEG_INFINITY, |info| info.last_teleport_time);
    if local_player_info.is_dead || commit_held(time.elapsed_secs(), last_teleport_time) {
        return;
    }
    local_player_info.move_seq = local_player_info.move_seq.wrapping_add(1);
    let _ = to_server.send(ClientToServer::Send(ClientMessage::Move(CMove {
        seq: local_player_info.move_seq,
        input: PlayerInput {
            move_intent: *move_intent,
            face_yaw: face_yaw.0,
        },
    })));
}

// Why the hold exists is at `COMMIT_TELEPORT_HOLD_SECS`.
fn commit_held(now: f32, last_teleport_time: f32) -> bool {
    now - last_teleport_time < COMMIT_TELEPORT_HOLD_SECS
}

// After this tick's movement, remember where the newest `CMove` left us,
// under its sequence, for the server's echo of that `CMove` to be measured
// against. Runs after the portal transit so a crossing is in the record the
// way it is in the server's.
pub fn record_committed_position_system(
    mut local_player_info: ResMut<LocalPlayerInfo>,
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
) {
    let Ok(pos) = local_player_query.single() else {
        return;
    };
    let seq = local_player_info.move_seq;
    local_player_info.committed_positions.record(seq, *pos);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_holds_right_after_a_local_hop() {
        assert!(commit_held(10.0, 10.0));
        assert!(commit_held(10.0 + COMMIT_TELEPORT_HOLD_SECS / 2.0, 10.0));
    }

    #[test]
    fn commit_resumes_once_the_hold_elapses() {
        assert!(!commit_held(10.0 + COMMIT_TELEPORT_HOLD_SECS, 10.0));
    }

    #[test]
    fn commit_runs_before_any_hop() {
        assert!(!commit_held(0.0, f32::NEG_INFINITY));
    }
}
