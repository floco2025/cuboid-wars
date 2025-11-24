use bevy::prelude::*;

#[allow(clippy::wildcard_imports)]
use common::protocol::*;

// ============================================================================
// Bevy Events for Network Messages
// ============================================================================

/// Event fired when the server sends a message to the client
#[derive(Event)]
pub struct ServerMessageReceived {
    pub message: ServerMessage,
}

/// Event fired when the client disconnects from the server
#[derive(Event)]
pub struct ServerDisconnected;
