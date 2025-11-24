use bevy::prelude::*;

// ============================================================================
// Bevy Events for Network Messages
// ============================================================================

/// Event fired when the client disconnects from the server
#[derive(Event)]
pub struct ServerDisconnected;
