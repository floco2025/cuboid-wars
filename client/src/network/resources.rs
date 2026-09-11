use bevy::prelude::*;
use std::{collections::VecDeque, time::Duration};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError};

use common::protocol::{ClientMessage, ServerMessage, sequence_is_newer};

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
    pub measurements: VecDeque<Duration>,
}

// The app's sending end of the client-to-server queue.
#[derive(Resource)]
pub struct ClientToServerChannel(UnboundedSender<ClientMessage>);

impl ClientToServerChannel {
    #[must_use]
    pub const fn new(sender: UnboundedSender<ClientMessage>) -> Self {
        Self(sender)
    }

    // The receiver is the network task or the host's own server; once it is
    // gone the link is closing and there is nobody left to tell.
    pub fn send(&self, message: ClientMessage) {
        let _ = self.0.send(message);
    }
}

// The app's receiving end of the server-to-client queue; a closed queue is
// the disconnect.
#[derive(Resource)]
pub struct ServerToClientChannel(UnboundedReceiver<ServerMessage>);

impl ServerToClientChannel {
    #[must_use]
    pub const fn new(receiver: UnboundedReceiver<ServerMessage>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<ServerMessage, TryRecvError> {
        self.0.try_recv()
    }
}

#[cfg(test)]
#[path = "tests/resources.rs"]
mod tests;
