use bevy::prelude::*;
use common::{
    math::angle_delta_radians,
    protocol::{CMove, ClientMessage, FaceYaw, PlayerMoveIntent},
};

use crate::{
    constants::ANGLE_COMMIT_THRESHOLD_DEGREES,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker},
};

// Once per fixed tick, send the local player's steady-state input (move
// intent + facing) to the server, but only when any of it meaningfully
// changed since the last commit. Tick cadence gates the rate; the angle
// threshold filters mouse-sensor jitter. State transitions (idle ↔ moving,
// walk ↔ run) always commit. Jump is event-shaped and sent immediately by
// `input_movement_system` — not handled here.
pub fn commit_player_input_system(
    to_server: Res<ClientToServerChannel>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    local_player_query: Query<(&PlayerMoveIntent, &FaceYaw), With<LocalPlayerMarker>>,
) {
    let Ok((move_intent, face_yaw)) = local_player_query.single() else {
        return;
    };
    let current = (*move_intent, face_yaw.0);

    if move_should_commit(current, local_player_info.last_sent_move) {
        let (move_intent, face_yaw) = current;
        let _ = to_server.send(ClientToServer::Send(ClientMessage::Move(CMove {
            move_intent,
            face_yaw,
        })));
        local_player_info.last_sent_move = current;
    }
}

fn move_should_commit(current: (PlayerMoveIntent, f32), last: (PlayerMoveIntent, f32)) -> bool {
    move_intent_should_commit(current.0, last.0) || angle_should_commit(current.1, last.1)
}

fn move_intent_should_commit(current: PlayerMoveIntent, last: PlayerMoveIntent) -> bool {
    let active_changed = current.direction().is_some() != last.direction().is_some();
    let mode_changed = current.is_running() != last.is_running();
    if active_changed || mode_changed {
        return true;
    }
    match (current.direction(), last.direction()) {
        (Some(new), Some(old)) => angle_should_commit(new, old),
        _ => false,
    }
}

fn angle_should_commit(current: f32, last: f32) -> bool {
    angle_delta_radians(current, last).abs() >= ANGLE_COMMIT_THRESHOLD_DEGREES.to_radians()
}
