use bevy::prelude::*;
use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};
use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender,
    error::{SendError, TryRecvError},
};

use crate::net::{ClientToServer, ServerToClient};
use common::protocol::{GhostId, ItemId, PlayerId, Ramp, Roof, Wall};

// ============================================================================
// Bevy Resources
// ============================================================================

// Wall configuration received from server
#[derive(Resource)]
pub struct WallConfig {
    pub boundary_walls: Vec<Wall>,
    pub interior_walls: Vec<Wall>,
    pub all_walls: Vec<Wall>, // Pre-computed: boundary + interior
    pub roofs: Vec<Roof>,
    pub ramps: Vec<Ramp>,
    pub roof_edge_walls: Vec<Wall>, // Roof edges (prevent falling off)
}

impl WallConfig {
    /// Check if a world position (x, z) is on a roof
    pub fn is_position_on_roof(&self, x: f32, z: f32) -> bool {
        for roof in &self.roofs {
            let min_x = roof.x1.min(roof.x2);
            let max_x = roof.x1.max(roof.x2);
            let min_z = roof.z1.min(roof.z2);
            let max_z = roof.z1.max(roof.z2);

            if x >= min_x && x <= max_x && z >= min_z && z <= max_z {
                return true;
            }
        }
        false
    }
}

// My player ID assigned by the server
#[derive(Resource)]
pub struct MyPlayerId(pub PlayerId);

// Player information (client-side)
pub struct PlayerInfo {
    pub entity: Entity,
    pub hits: i32,
    pub name: String,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
    pub reflect_power_up: bool,
    pub phasing_power_up: bool,
    pub ghost_hunt_power_up: bool,
    pub stunned: bool,
}

// Map of all players (client-side source of truth)
#[derive(Resource, Default)]
pub struct PlayerMap(pub HashMap<PlayerId, PlayerInfo>);

// Item information (client-side)
pub struct ItemInfo {
    pub entity: Entity,
}

// Map of all items (client-side source of truth)
#[derive(Resource, Default)]
pub struct ItemMap(pub HashMap<ItemId, ItemInfo>);

// Ghost information (client-side)
pub struct GhostInfo {
    pub entity: Entity,
}

// Map of all ghosts (client-side source of truth)
#[derive(Resource, Default)]
pub struct GhostMap(pub HashMap<GhostId, GhostInfo>);

// Last received SUpdate sequence number
#[derive(Resource, Default)]
pub struct LastUpdateSeq(pub u32);

// FPS measurement tracking
#[derive(Resource, Default)]
pub struct FpsMeasurement {
    pub frame_count: u32,
    pub fps_timer: f32,
    pub fps: f32,
}

// Round-trip time to server
#[derive(Resource, Default)]
pub struct RoundTripTime {
    pub rtt: Duration,
    pub pending_sent_at: Duration,
    pub measurements: VecDeque<Duration>,
}

// Camera view mode
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum CameraViewMode {
    #[default]
    FirstPerson,
    TopDown,
}

// Roof rendering toggle
#[derive(Resource, PartialEq, Eq, Clone, Copy, Debug)]
pub struct RoofRenderingEnabled(pub bool);

impl Default for RoofRenderingEnabled {
    fn default() -> Self {
        Self(true) // Roofs enabled by default
    }
}

// Resource wrapper for the client to server channel
#[derive(Resource)]
pub struct ClientToServerChannel(UnboundedSender<ClientToServer>);

impl ClientToServerChannel {
    #[must_use]
    pub const fn new(sender: UnboundedSender<ClientToServer>) -> Self {
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
    #[must_use]
    pub const fn new(receiver: UnboundedReceiver<ServerToClient>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<ServerToClient, TryRecvError> {
        self.0.try_recv()
    }
}
