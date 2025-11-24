use bevy::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::net::ServerToClient;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;

// ============================================================================
// Bevy Messages
// ============================================================================

/// Event fired when a client establishes a connection
#[derive(Message)]
pub struct ClientConnected {
    pub id: PlayerId,
    pub channel: UnboundedSender<ServerToClient>,
}

/// Event fired when a client disconnects
#[derive(Message)]
pub struct ClientDisconnected {
    pub id: PlayerId,
}

/// Event fired when a client sends a message
#[derive(Message)]
pub struct ClientMessageReceived {
    pub id: PlayerId,
    pub message: ClientMessage,
}
