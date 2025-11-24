#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender,
    error::{SendError, TryRecvError},
};

use crate::net::{ClientToServer, ServerToClient};
use common::protocol::PlayerId;

// ============================================================================
// Client Resources
// ============================================================================

/// My player ID assigned by the server
#[derive(Resource)]
pub struct MyPlayerId(pub PlayerId);

/// Resource wrapper for the client to server channel
#[derive(Resource)]
pub struct ClientToServerChannel(UnboundedSender<ClientToServer>);

impl ClientToServerChannel {
    pub fn new(sender: UnboundedSender<ClientToServer>) -> Self {
        Self(sender)
    }

    pub fn send(&self, msg: ClientToServer) -> Result<(), SendError<ClientToServer>> {
        self.0.send(msg)
    }
}

/// Resource wrapper for the server to client channel
#[derive(Resource)]
pub struct ServerToClientChannel(UnboundedReceiver<ServerToClient>);

impl ServerToClientChannel {
    pub fn new(receiver: UnboundedReceiver<ServerToClient>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<ServerToClient, TryRecvError> {
        self.0.try_recv()
    }
}
