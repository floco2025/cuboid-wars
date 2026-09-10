use std::collections::VecDeque;

use bevy::prelude::*;
use common::protocol::PlayerMovementState;

#[derive(Default)]
pub struct LocalPortalCrossings {
    pub(crate) entrance: Option<PlayerMovementState>,
    pending: VecDeque<PendingPortalCrossing>,
}

struct PendingPortalCrossing {
    seq: u32,
    view_change: Vec2,
}

impl LocalPortalCrossings {
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(crate) fn record(&mut self, seq: u32, entrance: PlayerMovementState, view_change: Vec2) {
        self.entrance = Some(entrance);
        self.pending.push_back(PendingPortalCrossing { seq, view_change });
    }

    pub(crate) fn resolve(&mut self, seq: u32, accepted: bool) -> Option<Vec2> {
        if self.pending.front().is_none_or(|crossing| crossing.seq != seq) {
            return None;
        }
        if accepted {
            self.pending.pop_front();
            return Some(Vec2::ZERO);
        }
        let undo = self.pending.iter().map(|crossing| crossing.view_change).sum();
        self.clear();
        Some(undo)
    }

    pub fn clear(&mut self) {
        self.entrance = None;
        self.pending.clear();
    }
}
