use std::{ops::ControlFlow, time::Instant};

use anyhow::{Result, bail};
use bevy::prelude::Resource;
use crossbeam_channel::{Receiver, Sender, TryRecvError};

use common::protocol::{ClientMessage, ServerMessage};

use super::transport::RemoteLink;

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
