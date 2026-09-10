use bevy::prelude::*;
use common::{
    constants::PLAYER_MOVEMENT_TRUST_DISTANCE,
    math::{player_movement_is_trusted, worst_axis_divergence},
    protocol::{PlayerId, PlayerMovementState, sequence_is_newer},
};

use super::{PlayerInfo, PlayerMap, PlayerMovementReport};

pub(crate) fn queue_player_movement(id: PlayerId, report: PlayerMovementReport, players: &mut PlayerMap) {
    let Some(info) = players.get_mut(&id) else {
        return;
    };
    if info.life.portal_recovery_pending
        || !report.is_finite()
        || !sequence_is_newer(report.seq(), info.session.last_move_seq)
    {
        return;
    }
    info.session.last_move_seq = report.seq();
    info.life.pending_moves.push(report);
}

pub(crate) fn reconcile_player_movement(
    id: PlayerId,
    info: &mut PlayerInfo,
    seq: u32,
    reported: PlayerMovementState,
    movement: &mut PlayerMovementState,
) -> bool {
    info.life.processed_move_seq = Some(seq);
    let delta = Vec3::from(reported.pos) - Vec3::from(movement.pos);
    if player_movement_is_trusted(delta) {
        *movement = reported;
        return true;
    }
    let (axis, magnitude) = worst_axis_divergence(delta);
    warn!(
        "{}#{}, move {} rejected: |{}|={:.2} >= {:.2} (Δ x={:.2}, y={:.2}, z={:.2}); retaining server position",
        info.connection.name, id.0, seq, axis, magnitude, PLAYER_MOVEMENT_TRUST_DISTANCE, delta.x, delta.y, delta.z
    );
    false
}
