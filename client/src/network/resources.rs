use bevy::prelude::*;
use std::{collections::VecDeque, time::Duration};
use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender,
    error::{SendError, TryRecvError},
};

use super::transport::{ClientToServer, ServerToClient};

// Last applied `SSnapshot` sequence. Server messages can arrive out of order
// because each message uses its own QUIC unidirectional stream; older full
// snapshots must not roll the client back after a newer snapshot has applied.
#[derive(Resource, Default)]
pub struct LastSnapshotSeq(Option<SnapshotSeq>);

impl LastSnapshotSeq {
    #[must_use]
    pub fn should_accept(&self, seq: u32) -> bool {
        let seq = SnapshotSeq(seq);
        self.0.is_none_or(|last| seq.is_newer_than(last))
    }

    pub fn record(&mut self, seq: u32) {
        self.0 = Some(SnapshotSeq(seq));
    }

    #[must_use]
    pub fn last_raw(&self) -> Option<u32> {
        self.0.map(|seq| seq.0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct SnapshotSeq(u32);

impl SnapshotSeq {
    #[must_use]
    const fn is_newer_than(self, other: Self) -> bool {
        self.0 != other.0 && self.0.wrapping_sub(other.0) < (1 << 31)
    }
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
    fn snapshot_sequence_accepts_first_value() {
        let seq = LastSnapshotSeq::default();
        assert!(seq.should_accept(0));
    }

    #[test]
    fn snapshot_sequence_rejects_older_values() {
        let mut seq = LastSnapshotSeq::default();
        seq.record(10);

        assert!(!seq.should_accept(9));
        assert!(!seq.should_accept(10));
        assert!(seq.should_accept(11));
    }

    #[test]
    fn snapshot_sequence_wraps_forward() {
        let mut seq = LastSnapshotSeq::default();
        seq.record(u32::MAX);

        assert!(seq.should_accept(0));
        assert!(seq.should_accept(1));
        assert!(!seq.should_accept(u32::MAX - 1));
    }
}
