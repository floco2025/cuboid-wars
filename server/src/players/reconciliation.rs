use bevy::prelude::*;
use common::{
    constants::PLAYER_MOVEMENT_TRUST_DISTANCE,
    math::{player_movement_is_trusted, worst_axis_divergence},
    protocol::{PlayerId, PlayerMovementState},
};

use super::PlayerInfo;

pub(super) fn reconcile_player_movement(
    id: PlayerId,
    info: &mut PlayerInfo,
    movement: &mut PlayerMovementState,
) -> bool {
    let Some(report) = info.life.pending_move.take() else {
        return false;
    };
    info.life.processed_move_seq = Some(report.seq);
    let delta = Vec3::from(report.movement.pos) - Vec3::from(movement.pos);
    if player_movement_is_trusted(delta) {
        *movement = report.movement;
        info.session.hops = report.result_hops;
        return true;
    }
    let (axis, magnitude) = worst_axis_divergence(delta);
    warn!(
        "{}#{}, move {} rejected: |{}|={:.2} >= {:.2} (Δ x={:.2}, y={:.2}, z={:.2}); retaining server position",
        info.connection.name,
        id.0,
        report.seq,
        axis,
        magnitude,
        PLAYER_MOVEMENT_TRUST_DISTANCE,
        delta.x,
        delta.y,
        delta.z
    );
    false
}
