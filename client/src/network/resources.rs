use bevy::prelude::*;
use std::{collections::VecDeque, time::Duration};
use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender,
    error::{SendError, TryRecvError},
};

use super::transport::{ClientToServer, ServerToClient};

// Last received SSnapshot sequence number.
#[derive(Resource, Default)]
pub struct LastSnapshotSeq(pub u32);

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
