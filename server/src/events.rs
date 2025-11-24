use bevy::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

#[allow(clippy::wildcard_imports)]
use common::protocol::*;

use crate::net::ServerToClient;

// ============================================================================
// Bevy Events for Network Messages
// ============================================================================

/// Event fired when a client establishes a connection
#[derive(Event)]
pub struct ClientConnected {
    pub id: PlayerId,
    pub channel: UnboundedSender<ServerToClient>,
}

/// Event fired when a client disconnects
#[derive(Event)]
pub struct ClientDisconnected {
    pub id: PlayerId,
}

/// Event fired when a client sends a message
#[derive(Event)]
pub struct ClientMessageReceived {
    pub id: PlayerId,
    pub message: ClientMessage,
}
