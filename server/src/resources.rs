use bevy::prelude::*;
use std::collections::HashMap;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError};

use crate::net::{ClientToServer, ServerToClient};
use common::protocol::*;

// ============================================================================
// Bevy Resources
// ============================================================================

// Wall configuration - generated once at server startup
#[derive(Resource)]
pub struct WallConfig {
    pub walls: Vec<Wall>,
}

// Player information (server-side)
pub struct PlayerInfo {
    pub entity: Entity,
    pub logged_in: bool,
    pub channel: UnboundedSender<ServerToClient>,
    pub hits: i32,
    pub name: String,
}

// Map of all players (server-side source of truth)
#[derive(Resource, Default)]
pub struct PlayerMap(pub HashMap<PlayerId, PlayerInfo>);

// Resource wrapper for the channel from the accept connections task, which gives us the channel to
// send from thee server to the client.
#[derive(Resource)]
pub struct FromAcceptChannel(UnboundedReceiver<(PlayerId, UnboundedSender<ServerToClient>)>);

impl FromAcceptChannel {
    #[must_use]
    pub const fn new(receiver: UnboundedReceiver<(PlayerId, UnboundedSender<ServerToClient>)>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<(PlayerId, UnboundedSender<ServerToClient>), TryRecvError> {
        self.0.try_recv()
    }
}

// Resource wrapper for the channel from all per client network I/O tasks.ß
#[derive(Resource)]
pub struct FromClientsChannel(UnboundedReceiver<(PlayerId, ClientToServer)>);

impl FromClientsChannel {
    #[must_use]
    pub const fn new(receiver: UnboundedReceiver<(PlayerId, ClientToServer)>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<(PlayerId, ClientToServer), TryRecvError> {
        self.0.try_recv()
    }
}
