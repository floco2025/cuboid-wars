use std::collections::HashMap;

use anyhow::Result;
use bevy_ecs::prelude::Component;
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::validation::{validate_non_negative_finite, validate_positive_finite};

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
    pub jump_speed: f32,
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

    // Field ranges only; which actor kinds must appear is the caller's rule.
    pub fn validate(&self, path: &str) -> Result<()> {
        self.player.validate(&format!("{path}.player"))?;
        for (kind, actor) in &self.actors {
            actor.validate(&format!("{path}.actors.{kind}"))?;
        }
        validate_positive_finite(self.missile_speed, &format!("{path}.missile_speed"))?;
        validate_positive_finite(self.projectile_speed, &format!("{path}.projectile_speed"))?;
        validate_positive_finite(self.gravity, &format!("{path}.gravity"))?;
        validate_non_negative_finite(self.low_gravity, &format!("{path}.low_gravity"))?;
        validate_positive_finite(self.ladder_climb_ratio, &format!("{path}.ladder_climb_ratio"))?;
        self.knockback.validate(&format!("{path}.knockback"))
    }
}

impl PlayerMovementConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.walk_speed, &format!("{path}.walk_speed"))?;
        validate_positive_finite(self.run_speed, &format!("{path}.run_speed"))?;
        validate_positive_finite(self.speed_power_up, &format!("{path}.speed_power_up"))?;
        validate_positive_finite(self.jump_speed, &format!("{path}.jump_speed"))
    }
}

impl ActorMovementConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.roam_speed, &format!("{path}.roam_speed"))?;
        validate_positive_finite(self.active_speed, &format!("{path}.active_speed"))
    }
}

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
pub struct KnockbackConfig {
    pub max_speed: f32,
    pub up_speed: f32,
    pub deceleration: f32,
}

impl KnockbackConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.max_speed, &format!("{path}.max_speed"))?;
        validate_non_negative_finite(self.up_speed, &format!("{path}.up_speed"))?;
        validate_positive_finite(self.deceleration, &format!("{path}.deceleration"))
    }
}
