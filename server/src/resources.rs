#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use std::collections::HashMap;
use tokio::sync::mpsc::{UnboundedReceiver, error::TryRecvError};

use crate::net::ClientToServer;
use common::protocol::*;

// ============================================================================
// Bevy Resources
// ============================================================================

/// Index from PlayerId to Entity for fast lookups
#[derive(Resource, Default)]
pub struct PlayerIndex(pub HashMap<PlayerId, Entity>);

/// Pending messages to be processed after entities are spawned
#[derive(Resource, Default)]
pub struct PendingMessages(pub Vec<(PlayerId, ClientMessage)>);

/// Resource wrapper for the per client network I/O task to server channel
#[derive(Resource)]
pub struct ClientsToServerChannel(pub UnboundedReceiver<(PlayerId, ClientToServer)>);

impl ClientsToServerChannel {
    pub fn new(receiver: UnboundedReceiver<(PlayerId, ClientToServer)>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<(PlayerId, ClientToServer), TryRecvError> {
        self.0.try_recv()
    }
}
