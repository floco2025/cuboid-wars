#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use tokio::sync::mpsc::{UnboundedSender, error::SendError};

use crate::net::ServerToClient;

// ============================================================================
// Bevy Components
// ============================================================================

/// Network channel for sending messages to a specific client
#[derive(Component)]
pub struct ServerToClientChannel(UnboundedSender<ServerToClient>);

impl ServerToClientChannel {
    pub fn new(sender: UnboundedSender<ServerToClient>) -> Self {
        Self(sender)
    }

    pub fn send(&self, message: ServerToClient) -> Result<(), SendError<ServerToClient>> {
        self.0.send(message)
    }
}

/// Marker component: client is logged in (authenticated)
#[derive(Component)]
pub struct LoggedIn;
