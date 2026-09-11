use bevy::prelude::*;
use std::{
    collections::VecDeque,
    ops::ControlFlow,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use crossbeam_channel::{Receiver, Sender, TryRecvError};

use common::protocol::{ClientMessage, ServerMessage, sequence_is_newer};

use super::transport::RemoteLink;

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
pub struct ClientToServerChannel(Sender<ClientMessage>);

impl ClientToServerChannel {
    #[must_use]
    pub const fn new(sender: Sender<ClientMessage>) -> Self {
        Self(sender)
    }

    // The receiver is the remote link or the host's own server; once it is
    // gone the link is closing and there is nobody left to tell.
    pub fn send(&self, message: ClientMessage) {
        let _ = self.0.send(message);
    }
}

// The app's receiving side of the server link: the host's own client reads
// its server's queue, a joined client pumps the UDP connection.
#[derive(Resource)]
pub enum ServerLink {
    Local(Receiver<ServerMessage>),
    Remote(Box<RemoteLink>),
}

impl ServerLink {
    // Hands the messages that have arrived to `sink` until it breaks; the
    // rest wait for the next call. An error is the link closing, with the
    // reason.
    pub fn receive(&mut self, now: Instant, mut sink: impl FnMut(ServerMessage) -> ControlFlow<()>) -> Result<()> {
        match self {
            Self::Local(receiver) => loop {
                match receiver.try_recv() {
                    Ok(message) => {
                        if sink(message).is_break() {
                            return Ok(());
                        }
                    }
                    Err(TryRecvError::Empty) => return Ok(()),
                    Err(TryRecvError::Disconnected) => bail!("the server closed the queue"),
                }
            },
            Self::Remote(link) => link.receive(now, sink),
        }
    }

    pub fn flush(&mut self, now: Instant) {
        if let Self::Remote(link) = self {
            link.flush(now);
        }
    }

    pub fn disconnect(&mut self) {
        if let Self::Remote(link) = self {
            link.disconnect();
        }
    }
}

#[cfg(test)]
#[path = "tests/resources.rs"]
mod tests;
