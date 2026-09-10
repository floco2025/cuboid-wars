use std::collections::VecDeque;

use common::protocol::{CMove, CPortalCross, CPortalRecovery, PlayerId, PlayerMovementState, sequence_is_newer};

use super::PlayerMap;

pub(crate) enum PlayerMovementReport {
    Move(CMove),
    PortalCross(CPortalCross),
}

impl PlayerMovementReport {
    pub fn seq(&self) -> u32 {
        match self {
            Self::Move(report) => report.seq,
            Self::PortalCross(report) => report.seq,
        }
    }

    // The state the server's own step is compared against: a crossing is
    // judged on its entrance side.
    pub fn comparison_state(&self) -> &PlayerMovementState {
        match self {
            Self::Move(report) => &report.movement,
            Self::PortalCross(report) => &report.entrance,
        }
    }

    fn is_finite(&self) -> bool {
        match self {
            Self::Move(report) => report.movement.is_finite(),
            Self::PortalCross(report) => report.entrance.is_finite() && report.movement.is_finite(),
        }
    }
}

// Reports waiting for a tick to process them, newest last.
#[derive(Default)]
pub(crate) struct PlayerMovementReports(VecDeque<PlayerMovementReport>);

impl PlayerMovementReports {
    pub fn push(&mut self, report: PlayerMovementReport) {
        // Coalescing may skip ordinary movement, but never a crossing boundary.
        if matches!(report, PlayerMovementReport::Move(_))
            && matches!(self.0.back(), Some(PlayerMovementReport::Move(_)))
        {
            self.0.pop_back();
        }
        self.0.push_back(report);
    }

    pub fn front(&self) -> Option<&PlayerMovementReport> {
        self.0.front()
    }

    pub fn pop_front(&mut self) -> Option<PlayerMovementReport> {
        self.0.pop_front()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

// Admission: a report is queued only while no rejected crossing awaits its
// recovery, and only when it is newer than every report admitted before it.
// A non-finite value is refused here so it can never reach the state the
// server broadcasts to everyone.
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

pub(crate) fn handle_portal_recovery_message(id: PlayerId, recovery: CPortalRecovery, players: &mut PlayerMap) {
    let Some(info) = players.get_mut(&id) else {
        return;
    };
    if info.life.portal_recovery_pending && !sequence_is_newer(info.session.last_move_seq, recovery.seq) {
        info.session.last_move_seq = recovery.seq;
        info.life.portal_recovery_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::{PlayerMoveIntent, Position};

    fn report(seq: u32) -> PlayerMovementReport {
        let movement = PlayerMovementState::new(Position::default(), PlayerMoveIntent::Idle, 0.0, 0.0);
        PlayerMovementReport::Move(CMove { seq, movement })
    }

    fn crossing(seq: u32) -> PlayerMovementReport {
        let movement = PlayerMovementState::new(Position::default(), PlayerMoveIntent::Idle, 0.0, 0.0);
        PlayerMovementReport::PortalCross(CPortalCross {
            seq,
            entrance: movement,
            movement,
        })
    }

    #[test]
    fn consecutive_moves_coalesce_but_crossings_keep_their_order() {
        let mut reports = PlayerMovementReports::default();
        reports.push(report(1));
        reports.push(report(2));
        reports.push(crossing(3));
        reports.push(report(4));
        reports.push(report(5));
        reports.push(crossing(6));
        reports.push(crossing(7));
        let mut order = Vec::new();
        while let Some(report) = reports.pop_front() {
            order.push(report.seq());
        }
        assert_eq!(order, [2, 3, 5, 6, 7]);
    }
}
