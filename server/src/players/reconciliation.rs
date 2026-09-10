use bevy::prelude::*;
use common::protocol::{CPortalCross, PlayerId, PlayerMovementState, SPortalCrossed};

use super::PlayerInfo;

// The trust rule: the reported state replaces the server's whole result
// when the two positions agree, and the return says so. The report's intent
// and facing were applied before the step either way.
#[must_use]
pub(crate) fn adopt_trusted_movement(
    id: PlayerId,
    info: &PlayerInfo,
    seq: u32,
    reported: PlayerMovementState,
    movement: &mut PlayerMovementState,
) -> bool {
    let divergence = reported.divergence_from(movement);
    if divergence.within_limit() {
        *movement = reported;
        return true;
    }
    warn!(
        "{}#{}, move {seq} rejected, {divergence}; retaining server position",
        info.connection.name, id.0
    );
    false
}

// A crossing is judged on its entrance side and, once accepted, continues
// from the reported exit. A rejection holds every further report until the
// owner's recovery, since they all continue from the rejected exit.
pub(crate) fn resolve_portal_crossing(
    id: PlayerId,
    info: &mut PlayerInfo,
    report: &CPortalCross,
    movement: &mut PlayerMovementState,
    tick: u32,
) -> SPortalCrossed {
    let accepted = adopt_trusted_movement(id, info, report.seq, report.entrance, movement);
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
