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
use common::protocol::{ItemId, PlayerId, SentryId, Speed, SpeedLevel};

// ============================================================================
// Bevy Resources
// ============================================================================

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
    pub phasing_power_up: bool,
    pub sentry_hunt_power_up: bool,
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

// Sentry information (client-side)
pub struct SentryInfo {
    pub entity: Entity,
}

// Map of all sentries (client-side source of truth)
#[derive(Resource, Default)]
pub struct SentryMap(pub HashMap<SentryId, SentryInfo>);

// Last received SUpdate sequence number
#[derive(Resource, Default)]
pub struct LastUpdateSeq(pub u32);

// Client-only local player state (not synced)
#[derive(Resource)]
pub struct LocalPlayerInfo {
    pub last_shot_time: f32,
    pub last_sent_speed: Speed,
    pub last_sent_face: f32,
    pub last_send_speed_time: f32,
    pub last_send_face_time: f32,
    pub stored_yaw: f32,
    pub stored_pitch: f32,
}

impl Default for LocalPlayerInfo {
    fn default() -> Self {
        Self {
            last_shot_time: f32::NEG_INFINITY,
            last_sent_speed: Speed {
                speed_level: SpeedLevel::Idle,
                move_dir: 0.0,
            },
            last_sent_face: 0.0,
            last_send_speed_time: 0.0,
            last_send_face_time: 0.0,
            stored_yaw: 0.0,
            stored_pitch: 0.0,
        }
    }
}

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

// Input settings
#[derive(Resource, Clone, Copy, Debug)]
pub struct InputSettings {
    pub invert_pitch: bool,
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
