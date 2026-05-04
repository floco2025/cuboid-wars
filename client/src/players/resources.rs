use bevy::prelude::*;
use std::collections::HashMap;

use common::protocol::{PlayerId, PlayerMoveIntent};

// My player ID assigned by the server.
#[derive(Resource)]
pub struct MyPlayerId(pub PlayerId);

// Player information (client-side).
pub struct PlayerInfo {
    pub entity: Entity,
    pub hits: i32,
    pub name: String,
    pub speed_power_up: bool,
    pub multi_shot_power_up: bool,
    pub phasing_power_up: bool,
    pub stunned: bool,
}

// Map of all players (client-side source of truth).
#[derive(Resource, Default)]
pub struct PlayerMap(pub HashMap<PlayerId, PlayerInfo>);

// Client-only local player state (not synced).
#[derive(Resource)]
pub struct LocalPlayerInfo {
    pub last_shot_time: f32,
    pub last_sent_move_intent: PlayerMoveIntent,
    pub last_sent_face: f32,
    pub last_send_input_time: f32,
    pub last_send_face_time: f32,
    pub stored_yaw: f32,
    pub stored_pitch: f32,
}

impl Default for LocalPlayerInfo {
    fn default() -> Self {
        Self {
            last_shot_time: f32::NEG_INFINITY,
            last_sent_move_intent: PlayerMoveIntent::default(),
            last_sent_face: 0.0,
            last_send_input_time: 0.0,
            last_send_face_time: 0.0,
            stored_yaw: 0.0,
            stored_pitch: 0.0,
        }
    }
}
