use bevy::prelude::*;
use common::{
    math::angle_delta_radians,
    protocol::{CFace, CPlayerMoveIntent, ClientMessage, FaceDirection, PlayerMoveIntent},
};

use crate::{
    constants::ANGLE_COMMIT_THRESHOLD_DEGREES,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker},
};

// Once per fixed tick, send the local player's current move-intent and
// facing direction to the server, but only when they meaningfully changed
// since the last commit. Tick cadence gates the rate; the angle threshold
// filters mouse-sensor jitter. State transitions (idle ↔ moving, walk ↔ run)
// always commit. Jump is event-shaped and sent immediately by
// `input_movement_system` — not handled here.
pub fn commit_player_input_system(
    to_server: Res<ClientToServerChannel>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    local_player_query: Query<(&PlayerMoveIntent, &FaceDirection), With<LocalPlayerMarker>>,
) {
    let Ok((move_intent, face_direction)) = local_player_query.single() else {
        return;
    };
    let move_intent = *move_intent;
    let face_yaw = face_direction.0;

    if move_intent_should_commit(move_intent, local_player_info.last_sent_move_intent) {
        let _ = to_server.send(ClientToServer::Send(ClientMessage::PlayerMoveIntent(
            CPlayerMoveIntent { move_intent },
        )));
        local_player_info.last_sent_move_intent = move_intent;
    }

    if face_should_commit(face_yaw, local_player_info.last_sent_face) {
        let _ = to_server.send(ClientToServer::Send(ClientMessage::Face(CFace { dir: face_yaw })));
        local_player_info.last_sent_face = face_yaw;
    }
}

fn move_intent_should_commit(current: PlayerMoveIntent, last: PlayerMoveIntent) -> bool {
    let active_changed = current.direction().is_some() != last.direction().is_some();
    let mode_changed = current.is_running() != last.is_running();
    if active_changed || mode_changed {
        return true;
    }
    match (current.direction(), last.direction()) {
        (Some(new), Some(old)) => angle_delta_radians(new, old).abs() >= ANGLE_COMMIT_THRESHOLD_DEGREES.to_radians(),
        _ => false,
    }
}

fn face_should_commit(current: f32, last: f32) -> bool {
    angle_delta_radians(current, last).abs() >= ANGLE_COMMIT_THRESHOLD_DEGREES.to_radians()
}
