use common::protocol::{CarrierId, Position};

// In the parent's frame, so a moving parent never leaves a dock stale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarrierDock {
    pub parent: CarrierId,
    pub position: Position,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TraversalAction {
    Walk {
        carrier: CarrierId,
        target: Position,
    },
    MountLadder {
        carrier: CarrierId,
        ladder: usize,
        target: Position,
    },
    Climb {
        carrier: CarrierId,
        ladder: usize,
        target: Position,
        normal: [f32; 2],
        ascending: bool,
    },
    ExitLadder {
        carrier: CarrierId,
        ladder: usize,
        target: Position,
    },
    WaitForDock {
        carrier: CarrierId,
        dock: CarrierDock,
    },
    Board {
        carrier: CarrierId,
        target: Position,
        dock: CarrierDock,
    },
    Ride {
        carrier: CarrierId,
        dock: CarrierDock,
    },
}

impl TraversalAction {
    pub(crate) fn committed(self) -> bool {
        self.ladder().is_some() || matches!(self, Self::Board { .. } | Self::Ride { .. })
    }
    pub(crate) fn ladder(self) -> Option<usize> {
        match self {
            Self::MountLadder { ladder, .. } | Self::Climb { ladder, .. } | Self::ExitLadder { ladder, .. } => {
                Some(ladder)
            }
            _ => None,
        }
    }
}
