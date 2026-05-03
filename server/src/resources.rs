use bevy::prelude::*;
use std::collections::HashMap;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError};

use crate::net::{ClientToServer, ServerToClient};
use common::{
    constants::{ALWAYS_MULTI_SHOT, ALWAYS_PHASING, ALWAYS_SPEED},
    protocol::*,
};

// ============================================================================
// Bevy Resources
// ============================================================================

// Cell flags.
#[derive(Copy, Clone, Debug, Default)]
pub struct Cell {
    pub has_ramp: bool,            // Cell occupied by a ramp footprint
    pub has_ramp_from_below: bool, // Cell has a ramp surface rising into this level
    pub has_floor: bool,           // Cell has regular floor eligible for automatic placement
    pub has_floor_slab: bool,      // Cell has any physical floor slab on this level
    pub has_floor_above: bool,     // Cell has any physical floor slab on the next level above it
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

impl Cell {
    // Spawn zones may target any cell that is not an obstruction and not an
    // inaccessible-floor slab. Empty cells (no floor at all) are fine — kinds
    // like flying actors don't need a floor underfoot. As new obstruction
    // flags are added (e.g. holes, hazards), gate them in here so the rule
    // stays in one place.
    #[must_use]
    pub fn is_spawnable(&self) -> bool {
        if self.has_ramp {
            return false;
        }
        // Inaccessible-floor: a slab exists but it's not a regular walkable
        // floor. Treat as forbidden.
        if self.has_floor_slab && !self.has_floor {
            return false;
        }
        true
    }
}

#[derive(Clone, Debug)]
pub struct CellGrid {
    pub rows: Vec<Vec<Cell>>,
}

impl CellGrid {
    #[must_use]
    pub fn new(grid_cols: i32, grid_rows: i32) -> Self {
        Self {
            rows: vec![vec![Cell::default(); grid_cols as usize]; grid_rows as usize],
        }
    }
}

// Edges live on grid lines, not in cells.
//
// horizontal[row][col] covers the horizontal segment from grid point
// (col, row) to (col + 1, row), so its dimensions are
// (grid_rows + 1) x grid_cols.
//
// vertical[row][col] covers the vertical segment from grid point
// (col, row) to (col, row + 1), so its dimensions are
// grid_rows x (grid_cols + 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeGrid {
    pub horizontal: Vec<Vec<bool>>,
    pub vertical: Vec<Vec<bool>>,
}

impl EdgeGrid {
    #[must_use]
    pub fn new(grid_cols: i32, grid_rows: i32) -> Self {
        Self {
            horizontal: vec![vec![false; grid_cols as usize]; grid_rows as usize + 1],
            vertical: vec![vec![false; grid_cols as usize + 1]; grid_rows as usize],
        }
    }
}

#[derive(Clone, Debug)]
pub struct LevelGrid {
    pub cells: CellGrid,
    pub edges: EdgeGrid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnEntry {
    pub kind: String,
    pub count: u32,
}

// Where actor entities are spawned. Each zone holds an entry list keyed by
// actor kind; the quota system keeps `count` of each kind alive in the zone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorSpawnZone {
    pub level: u8,
    // Half-open ranges: cols=[start, end), rows=[start, end). Always end > start.
    pub cols: [i32; 2],
    pub rows: [i32; 2],
    pub spawns: Vec<SpawnEntry>,
}

impl ActorSpawnZone {
    pub fn entry_for(&self, kind: &str) -> Option<&SpawnEntry> {
        self.spawns.iter().find(|e| e.kind == kind)
    }

    pub fn covers(&self, kind: &str) -> bool {
        self.entry_for(kind).is_some()
    }

    pub fn cells(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let cols = self.cols[0]..self.cols[1];
        let rows = self.rows[0]..self.rows[1];
        rows.flat_map(move |r| cols.clone().map(move |c| (c, r)))
    }
}

// Where players spawn (login + fall recovery). No quota, no kind list — the
// picker just chooses any spawnable cell from any player zone uniformly at
// random.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerSpawnZone {
    pub level: u8,
    pub cols: [i32; 2],
    pub rows: [i32; 2],
}

impl PlayerSpawnZone {
    pub fn cells(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let cols = self.cols[0]..self.cols[1];
        let rows = self.rows[0]..self.rows[1];
        rows.flat_map(move |r| cols.clone().map(move |c| (c, r)))
    }
}

// Map configuration
#[derive(Resource, Clone)]
pub struct MapConfig {
    pub levels: Vec<LevelGrid>,
    pub actor_spawn_zones: Vec<ActorSpawnZone>,
    pub player_spawn_zones: Vec<PlayerSpawnZone>,
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
    pub stun_timer: f32,           // Remaining time stunned (0.0 = not stunned)
    pub last_shot_time: f32,       // Timestamp of last accepted shot (seconds)
}

impl PlayerInfo {
    #[must_use]
    pub fn new(entity: Entity, channel: UnboundedSender<ServerToClient>) -> Self {
        Self {
            entity,
            logged_in: false,
            channel,
            hits: 0,
            name: String::new(),
            speed_power_up_timer: 0.0,
            multi_shot_power_up_timer: 0.0,
            phasing_power_up_timer: 0.0,
            stun_timer: 0.0,
            last_shot_time: f32::NEG_INFINITY,
        }
    }

    #[must_use]
    pub fn has_speed(&self) -> bool {
        ALWAYS_SPEED || self.speed_power_up_timer > 0.0
    }

    #[must_use]
    pub fn has_multi_shot(&self) -> bool {
        ALWAYS_MULTI_SHOT || self.multi_shot_power_up_timer > 0.0
    }

    #[must_use]
    pub fn has_phasing(&self) -> bool {
        ALWAYS_PHASING || self.phasing_power_up_timer > 0.0
    }

    // Build status message from current power-up timers.
    #[must_use]
    pub fn status(&self, id: PlayerId) -> SPlayerStatus {
        SPlayerStatus {
            id,
            speed_power_up: self.has_speed(),
            multi_shot_power_up: self.has_multi_shot(),
            phasing_power_up: self.has_phasing(),
            stunned: self.stun_timer > 0.0,
        }
    }

    // Tick all power-up and status timers by delta, clamping to 0.
    pub fn tick_timers(&mut self, delta: f32) {
        tick_timer(&mut self.speed_power_up_timer, delta);
        tick_timer(&mut self.multi_shot_power_up_timer, delta);
        tick_timer(&mut self.phasing_power_up_timer, delta);
        tick_timer(&mut self.stun_timer, delta);
    }
}

fn tick_timer(timer: &mut f32, delta: f32) {
    *timer = (*timer - delta).max(0.0);
}

// Map of all players (server-side source of truth)
#[derive(Resource, Default)]
pub struct PlayerMap(pub HashMap<PlayerId, PlayerInfo>);

// Actor information (server-side)
pub struct ActorInfo {
    pub entity: Entity,
    pub kind: ActorKind,
    pub spawn_zone_index: usize,
    pub spawn_kind: String,
    pub direction_timer: f32,
    pub patrol_intent: ActorMoveIntent,
    pub go_to_position: Option<Position>,
    pub wall_avoidance_direction: Option<f32>,
    pub last_broadcast_move_intent: ActorMoveIntent,
    pub move_intent_send_timer: f32,
}

// Map of all server-controlled actors.
#[derive(Resource, Default)]
pub struct ActorMap(pub HashMap<ActorId, ActorInfo>);

// Item information (server-side)
pub struct ItemInfo {
    pub entity: Entity,
    pub item_type: ItemType,
    pub spawn_time: f32, // For power-ups: spawn time; For cookies: respawn countdown (0.0 = available)
}

// Map of all items (server-side source of truth)
#[derive(Resource, Default)]
pub struct ItemMap(pub HashMap<ItemId, ItemInfo>);

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

// Actor spawner: allocates monotonically increasing actor ids.
#[derive(Resource, Default)]
pub struct ActorSpawner {
    pub next_id: u32,
}

// Per-zone spawn throttle in seconds. Keyed by zone index. The value is the
// time remaining until the next spawn into that zone is permitted. The
// throttle only ticks down while the zone is short of its quota
// (live < count); when the zone fills, the throttle freezes at whatever
// value it has (typically `spawn_throttle_seconds` from the most recent
// spawn). Zones that have never spawned have no entry, which is treated
// as "go ahead immediately" — the spawner inserts 0.0 on the first tick
// so the throttle then runs for subsequent spawns.
//
// Note: today there's only one actor kind per zone. When multiple kinds
// per zone arrive, key on `(usize, ActorKind)` (or similar) instead of
// just zone index so each kind has its own throttle slot.
#[derive(Resource, Default)]
pub struct ActorSpawnThrottles(pub HashMap<usize, f32>);

impl ActorSpawner {
    pub fn allocate(&mut self) -> ActorId {
        let id = ActorId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }
}
