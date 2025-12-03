use bevy::prelude::*;
use std::collections::HashMap;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError};

use crate::net::{ClientToServer, ServerToClient};
use common::protocol::*;

// ============================================================================
// Bevy Resources
// ============================================================================

// Grid cell wall edges - bitflags for efficient lookup
#[derive(Debug, Clone, Copy, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct GridCell {
    pub has_north_wall: bool, // Horizontal wall at top edge (z)
    pub has_south_wall: bool, // Horizontal wall at bottom edge (z+1)
    pub has_west_wall: bool,  // Vertical wall at left edge (x)
    pub has_east_wall: bool,  // Vertical wall at right edge (x+1)
}

// Grid configuration - generated once at server startup
#[derive(Resource)]
pub struct GridConfig {
    pub walls: Vec<Wall>,
    pub roofs: Vec<Roof>,
    pub grid: Vec<Vec<GridCell>>, // [row][col] - indexed by grid_z, grid_x
}

// Player information (server-side)
pub struct PlayerInfo {
    pub entity: Entity,
    pub logged_in: bool,
    pub channel: UnboundedSender<ServerToClient>,
    pub hits: i32,
    pub name: String,
    pub speed_power_up_timer: f32, // Remaining time for speed power-up (0.0 = inactive)
    pub multi_shot_power_up_timer: f32, // Remaining time for multi-shot power-up (0.0 = inactive)
    pub reflect_power_up_timer: f32, // Remaining time for reflect power-up (0.0 = inactive)
}

// Map of all players (server-side source of truth)
#[derive(Resource, Default)]
pub struct PlayerMap(pub HashMap<PlayerId, PlayerInfo>);

// Item information (server-side)
pub struct ItemInfo {
    pub entity: Entity,
    pub item_type: ItemType,
    pub spawn_time: f32,
}

// Map of all items (server-side source of truth)
#[derive(Resource, Default)]
pub struct ItemMap(pub HashMap<ItemId, ItemInfo>);

// Ghost AI mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhostMode {
    Patrol,           // Moving along grid, can detect players
    Follow,           // Following a specific player
    PatrolCooldown,   // Moving along grid, cannot detect players yet
}

// Ghost info
pub struct GhostInfo {
    pub entity: Entity,
    pub mode: GhostMode,
    pub mode_timer: f32,           // Time remaining in current mode
    pub follow_target: Option<PlayerId>, // Player being followed (only in Follow mode)
}

// Map of all ghosts (server-side source of truth)
#[derive(Resource, Default)]
pub struct GhostMap(pub HashMap<GhostId, GhostInfo>);

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

// Item spawner timer
#[derive(Resource)]
pub struct ItemSpawner {
    pub timer: f32,
    pub next_id: u32, // Next ItemId to assign
}

impl Default for ItemSpawner {
    fn default() -> Self {
        Self { timer: 0.0, next_id: 0 }
    }
}
