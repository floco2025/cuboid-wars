use common::protocol::{
    CPortalCross, CPortalRecovery, PlayerId, PlayerMovementState, SPortalCrossed, ServerMessage, sequence_is_newer,
};

use crate::{
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap, PlayerMovementReport, queue_player_movement, reconcile_player_movement},
};

pub(crate) fn handle_portal_cross_message(id: PlayerId, report: CPortalCross, players: &mut PlayerMap) {
    queue_player_movement(id, PlayerMovementReport::PortalCross(report), players);
}

pub(crate) fn handle_portal_recovery_message(id: PlayerId, recovery: CPortalRecovery, players: &mut PlayerMap) {
    let Some(info) = players.get_mut(&id) else {
        return;
    };
    if info.life.portal_recovery_pending && !sequence_is_newer(info.session.last_move_seq, recovery.seq) {
        info.session.last_move_seq = recovery.seq;
        info.life.portal_recovery_pending = false;
    }
}

pub(crate) fn resolve_portal_crossing(
    id: PlayerId,
    info: &mut PlayerInfo,
    report: &CPortalCross,
    movement: &mut PlayerMovementState,
    tick: u32,
) -> SPortalCrossed {
    let accepted = reconcile_player_movement(id, info, report.seq, report.entrance, movement);
    if accepted {
        *movement = report.movement;
        info.life.fall_state.reset();
    } else {
        info.life.pending_moves.clear();
        info.life.portal_recovery_pending = true;
    }
    SPortalCrossed {
        id,
        seq: report.seq,
        tick,
        accepted,
        movement: *movement,
    }
}

pub(crate) fn broadcast_portal_crossing(players: &PlayerMap, result: SPortalCrossed) {
    for (id, info) in players.iter() {
        if info.connection.logged_in && (result.accepted || *id == result.id) {
            let _ = info
                .connection
                .channel
                .send(ServerToClient::Send(ServerMessage::PortalCrossed(result)));
        }
    }
}
