use bevy::prelude::*;
use std::{collections::VecDeque, time::Duration};
use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender,
    error::{SendError, TryRecvError},
};

use super::transport::{ClientToServer, ServerToClient};

// Newest `SSnapshot.seq` applied; an older snapshot is ignored. 0 until
// the first one, since the server counts from 1.
#[derive(Resource, Default)]
pub struct LastSnapshotSeq(pub u32);

// Newest `SPlayerMoves.seq` applied; same contract as `LastSnapshotSeq`.
#[derive(Resource, Default)]
pub struct LastPlayerMovesSeq(pub u32);

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
    use common::protocol::sequence_is_newer;

    #[test]
    fn snapshot_sequence_wraps_forward() {
        let last = LastSnapshotSeq(u32::MAX);
        assert!(sequence_is_newer(1, LastSnapshotSeq::default().0));
        assert!(sequence_is_newer(0, last.0));
        assert!(!sequence_is_newer(u32::MAX - 1, last.0));
    }
}
