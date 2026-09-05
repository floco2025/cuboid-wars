use bevy::prelude::*;
use std::{collections::VecDeque, time::Duration};
use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender,
    error::{SendError, TryRecvError},
};

use common::protocol::sequence_is_newer;

use super::transport::{ClientToServer, ServerToClient};

// Newest `SSnapshot.tick` applied; an older snapshot is ignored. `None`
// until the first one.
#[derive(Resource, Default)]
pub struct LastSnapshotTick(pub Option<u32>);

// Newest `SPlayerMoves.tick` applied; same contract as `LastSnapshotTick`.
#[derive(Resource, Default)]
pub struct LastPlayerMovesTick(pub Option<u32>);

// Records `tick` as the newest applied and says whether it was newer than
// the last; a first tick is always newest.
pub fn accept_newer_tick(last: &mut Option<u32>, tick: u32) -> bool {
    if last.is_some_and(|last| !sequence_is_newer(tick, last)) {
        return false;
    }
    *last = Some(tick);
    true
}

// Round-trip time to server.
#[derive(Resource, Default)]
pub struct RoundTripTime {
    pub rtt: Duration,
    pub pending_sent_at: Duration,
    pub measurements: VecDeque<Duration>,
}

// Resource wrapper for the client to server channel.
#[derive(Resource)]
pub struct ClientToServerChannel(UnboundedSender<ClientToServer>);

impl ClientToServerChannel {
    #[must_use]
    pub const fn new(sender: UnboundedSender<ClientToServer>) -> Self {
        Self(sender)
    }

    pub fn send(&self, msg: ClientToServer) -> Result<(), SendError<ClientToServer>> {
        self.0.send(msg)
    }
}

// Resource wrapper for the server to client channel.
#[derive(Resource)]
pub struct ServerToClientChannel(UnboundedReceiver<ServerToClient>);

impl ServerToClientChannel {
    #[must_use]
    pub const fn new(receiver: UnboundedReceiver<ServerToClient>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<ServerToClient, TryRecvError> {
        self.0.try_recv()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_first_tick_is_accepted() {
        let mut last = None;
        assert!(accept_newer_tick(&mut last, u32::MAX - 3));
        assert_eq!(last, Some(u32::MAX - 3));
    }

    #[test]
    fn ticks_wrap_forward_and_older_ones_are_rejected() {
        let mut last = Some(u32::MAX);
        assert!(accept_newer_tick(&mut last, 0));
        assert!(!accept_newer_tick(&mut last, u32::MAX - 1));
        assert_eq!(last, Some(0));
    }
}
