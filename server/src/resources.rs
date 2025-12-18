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
pub struct GridCell {
    pub has_north_wall: bool, // Horizontal wall at top edge (z)
    pub has_south_wall: bool, // Horizontal wall at bottom edge (z+1)
    pub has_west_wall: bool,  // Vertical wall at left edge (x)
    pub has_east_wall: bool,  // Vertical wall at right edge (x+1)
    pub has_ramp: bool,       // Cell occupied by a ramp footprint
    pub has_roof: bool,       // Cell has a roof on top
    // Ramp bases disallow walls on their entry edge
    pub ramp_base_north: bool,
    pub ramp_base_south: bool,
    pub ramp_base_west: bool,
    pub ramp_base_east: bool,
    // Ramp tops disallow walls on their exit edge
    pub ramp_top_north: bool,
    pub ramp_top_south: bool,
    pub ramp_top_west: bool,
    pub ramp_top_east: bool,
}

// Grid configuration - generated once at server startup
#[derive(Resource)]
pub struct GridConfig {
    pub grid: Vec<Vec<GridCell>>, // [row][col] - indexed by grid_z, grid_x
    pub boundary_walls: Vec<Wall>,
    pub interior_walls: Vec<Wall>,
    pub all_walls: Vec<Wall>, // Pre-computed: boundary + interior
    pub roofs: Vec<Roof>,
    pub ramps: Vec<Ramp>,
    pub roof_edge_walls: Vec<Wall>, // Collision boxes for roof edges (prevent falling off)
}

impl GridConfig {
    /// Check if a world position (x, z) is on a roof cell
    pub fn is_position_on_roof(&self, x: f32, z: f32) -> bool {
        use common::constants::*;

        // Convert world coordinates to grid coordinates
        let col = ((x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32;
        let row = ((z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32;

        // Check bounds
        if row < 0 || row >= GRID_ROWS || col < 0 || col >= GRID_COLS {
            return false;
        }

        self.grid[row as usize][col as usize].has_roof
    }
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
    pub phasing_power_up_timer: f32, // Remaining time for phasing power-up (0.0 = inactive)
    pub ghost_hunt_power_up_timer: f32, // Remaining time for ghost hunt power-up (0.0 = inactive)
    pub stun_timer: f32,           // Remaining time stunned (0.0 = not stunned)
}

// Map of all players (server-side source of truth)
#[derive(Resource, Default)]
pub struct PlayerMap(pub HashMap<PlayerId, PlayerInfo>);

// Item information (server-side)
pub struct ItemInfo {
    pub entity: Entity,
    pub item_type: ItemType,
    pub spawn_time: f32, // For power-ups: spawn time; For cookies: respawn countdown (0.0 = available)
}

// Map of all items (server-side source of truth)
#[derive(Resource, Default)]
pub struct ItemMap(pub HashMap<ItemId, ItemInfo>);

// Ghost AI mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhostMode {
    PrePatrol, // Navigating to grid center before patrol
    Patrol,    // Moving along grid, can detect players (unless mode_timer > 0)
    Target,    // Targeting a specific player (chase or flee)
}

// Ghost info
pub struct GhostInfo {
    pub entity: Entity,
    pub mode: GhostMode,
    pub mode_timer: f32,                 // Time remaining in current mode
    pub follow_target: Option<PlayerId>, // Player being targeted (only in Target mode)
    pub at_intersection: bool,           // Track if currently at an intersection (for patrol mode)
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
