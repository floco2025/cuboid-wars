use bevy::prelude::*;

// ============================================================================
// Bevy Messages
// ============================================================================

/// Event fired when the client disconnects from the server
#[derive(Message)]
pub struct ServerDisconnected;
