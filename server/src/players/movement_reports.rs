use std::collections::VecDeque;

use common::protocol::{CMove, CPortalCross, PlayerMovementState};

#[derive(Debug)]
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

    pub fn comparison_state(&self) -> &PlayerMovementState {
        match self {
            Self::Move(report) => &report.movement,
            Self::PortalCross(report) => &report.entrance,
        }
    }

    pub fn is_finite(&self) -> bool {
        self.comparison_state().is_finite()
            && match self {
                Self::Move(_) => true,
                Self::PortalCross(report) => report.movement.is_finite(),
            }
    }
}

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

    pub fn pop(&mut self) -> Option<PlayerMovementReport> {
        self.0.pop_front()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}
