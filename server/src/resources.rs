use bevy::prelude::*;
use std::collections::HashMap;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError};

use crate::net::{ClientToServer, ServerToClient};
use common::{
    constants::{
        ALWAYS_MULTI_SHOT, ALWAYS_PHASING, ALWAYS_SENTRY_HUNT, ALWAYS_SPEED, FIELD_DEPTH, FIELD_WIDTH, GRID_COLS,
        GRID_ROWS, GRID_SIZE,
    },
    protocol::*,
};

// ============================================================================
// Bevy Resources
// ============================================================================

// Grid cell flags
#[derive(Copy, Clone, Debug, Default)]
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

// Grid configuration
#[derive(Resource, Clone)]
pub struct GridConfig {
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
    pub phasing_power_up_timer: f32, // Remaining time for phasing power-up (0.0 = inactive)
    pub sentry_hunt_power_up_timer: f32, // Remaining time for sentry hunter power-up (0.0 = inactive)
    pub stun_timer: f32,           // Remaining time stunned (0.0 = not stunned)
    pub last_shot_time: f32,       // Timestamp of last accepted shot (seconds)
}

impl PlayerInfo {
    // Build status message from current power-up timers.
    #[must_use]
    pub fn status(&self, id: PlayerId) -> SPlayerStatus {
        SPlayerStatus {
            id,
            speed_power_up: ALWAYS_SPEED || self.speed_power_up_timer > 0.0,
            multi_shot_power_up: ALWAYS_MULTI_SHOT || self.multi_shot_power_up_timer > 0.0,
            phasing_power_up: ALWAYS_PHASING || self.phasing_power_up_timer > 0.0,
            sentry_hunt_power_up: ALWAYS_SENTRY_HUNT || self.sentry_hunt_power_up_timer > 0.0,
            stunned: self.stun_timer > 0.0,
        }
    }

    // Tick all power-up and status timers by delta, clamping to 0.
    pub fn tick_timers(&mut self, delta: f32) {
        self.speed_power_up_timer = (self.speed_power_up_timer - delta).max(0.0);
        self.multi_shot_power_up_timer = (self.multi_shot_power_up_timer - delta).max(0.0);
        self.phasing_power_up_timer = (self.phasing_power_up_timer - delta).max(0.0);
        self.sentry_hunt_power_up_timer = (self.sentry_hunt_power_up_timer - delta).max(0.0);
        self.stun_timer = (self.stun_timer - delta).max(0.0);
    }
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

// Configuration for sentry spawning
#[derive(Resource)]
pub struct SentrySpawnConfig {
    pub num_sentries: u32,
}

// Sentry AI mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentryMode {
    PrePatrol, // Navigating to grid center before patrol
    Patrol,    // Moving along grid, can detect players (unless mode_timer > 0)
    Target,    // Targeting a specific player (chase or flee)
}

// Sentry info
pub struct SentryInfo {
    pub entity: Entity,
    pub mode: SentryMode,
    pub mode_timer: f32,                 // Time remaining in current mode
    pub follow_target: Option<PlayerId>, // Player being targeted (only in Target mode)
    pub at_intersection: bool,           // Track if currently at an intersection (for patrol mode)
}

// Map of all sentries (server-side source of truth)
#[derive(Resource, Default)]
pub struct SentryMap(pub HashMap<SentryId, SentryInfo>);

// Grid of cells showing which sentry occupies each cell (for collision avoidance)
// grid[z][x] = Some(SentryId) or None
#[derive(Resource, Clone)]
pub struct SentryGrid(pub Vec<Vec<Option<SentryId>>>);

impl SentryGrid {
    // Clear a sentry from both cells it occupies while patrolling.
    // A patrolling sentry occupies two adjacent cells along its axis of movement:
    // - If in first half (before cell center): current cell + cell in velocity direction
    // - If in second half (past cell center): current cell + cell opposite to velocity direction
    // Since determining which half is complex, we simply clear both adjacent cells along the
    // movement axis. The extra clear is harmless (no-op if cell doesn't contain this sentry).
    pub fn clear_patrol_cells(&mut self, pos: &Position, vel: &Velocity, sentry_id: SentryId) {
        let grid_x = (((pos.x + FIELD_WIDTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_COLS - 1);
        let grid_z = (((pos.z + FIELD_DEPTH / 2.0) / GRID_SIZE).floor() as i32).clamp(0, GRID_ROWS - 1);

        // Clear current cell
        if self.0[grid_z as usize][grid_x as usize] == Some(sentry_id) {
            self.0[grid_z as usize][grid_x as usize] = None;
        }

        // Clear both adjacent cells along the axis of movement
        if vel.x.abs() > 0.0 {
            // Moving East/West - clear both East and West neighbors
            self.clear_cell_if_matches(grid_x + 1, grid_z, sentry_id);
            self.clear_cell_if_matches(grid_x - 1, grid_z, sentry_id);
        } else if vel.z.abs() > 0.0 {
            // Moving North/South - clear both North and South neighbors
            self.clear_cell_if_matches(grid_x, grid_z + 1, sentry_id);
            self.clear_cell_if_matches(grid_x, grid_z - 1, sentry_id);
        }
        // If velocity is zero, sentry only occupies current cell (already cleared above)
    }

    // Helper: clear a cell if it contains the specified sentry and is in bounds
    fn clear_cell_if_matches(&mut self, grid_x: i32, grid_z: i32, sentry_id: SentryId) {
        if (0..GRID_COLS).contains(&grid_x)
            && (0..GRID_ROWS).contains(&grid_z)
            && self.0[grid_z as usize][grid_x as usize] == Some(sentry_id)
        {
            self.0[grid_z as usize][grid_x as usize] = None;
        }
    }
}

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
