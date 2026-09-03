use std::collections::HashMap;

use bevy_ecs::prelude::Component;
use bincode::{Decode, Encode};
use serde::Deserialize;

#[derive(Debug, Clone, Encode, Decode, Deserialize)]
pub struct MapMovementConfig {
    pub player: PlayerMovementConfig,
    pub actors: HashMap<String, ActorMovementConfig>,
    pub missile_speed: f32,
    pub projectile_speed: f32,
    // Positive gravity magnitudes (m/s²). `low_gravity` replaces `gravity`
    // while the low-gravity power-up is active.
    pub gravity: f32,
    pub low_gravity: f32,
    // Climb rate per unit of intent speed into (ascend) or away from
    // (descend) the ladder face.
    pub ladder_climb_ratio: f32,
    pub knockback: KnockbackConfig,
}

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
pub struct PlayerMovementConfig {
    pub walk_speed: f32,
    pub run_speed: f32,
    pub speed_power_up: f32,
}

// A component on server actors so movement ticks do not hash the kind string.
#[derive(Debug, Clone, Copy, Component, Encode, Decode, Deserialize)]
pub struct ActorMovementConfig {
    pub roam_speed: f32,
    pub active_speed: f32,
}

impl MapMovementConfig {
    #[must_use]
    pub fn expect_actor(&self, kind: &str) -> &ActorMovementConfig {
        self.actors.get(kind).expect("actor kind missing from movement.actors")
    }
}

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
pub struct KnockbackConfig {
    pub max_speed: f32,
    pub up_speed: f32,
    pub deceleration: f32,
}
