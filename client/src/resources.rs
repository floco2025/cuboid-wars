#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use std::collections::HashMap;
use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender,
    error::{SendError, TryRecvError},
};

use crate::net::{ClientToServer, ServerToClient};
use common::protocol::{PlayerId, Wall};

// ============================================================================
// Bevy Resources
// ============================================================================

// Wall configuration received from server
#[derive(Resource)]
pub struct WallConfig {
    pub walls: Vec<Wall>,
}

// My player ID assigned by the server
#[derive(Resource)]
pub struct MyPlayerId(pub PlayerId);

// Player information (client-side)
pub struct PlayerInfo {
    pub entity: Entity,
    pub hits: i32,
}

// Map of all players (client-side source of truth)
#[derive(Resource, Default)]
pub struct PlayerMap(pub HashMap<PlayerId, PlayerInfo>);

// Round-trip time to server in milliseconds
#[derive(Resource, Default)]
pub struct RoundTripTime {
    pub rtt_ms: u64,
    pub pending_timestamp: u64,
}

// Resource wrapper for the client to server channel
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

// Resource wrapper for the server to client channel
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
