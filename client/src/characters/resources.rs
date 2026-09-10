use std::collections::HashMap;

use bevy::prelude::*;
use common::protocol::Health;

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundsMode {
    #[default]
    Off,
    Grounding,
    Hitbox,
}

impl BoundsMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Grounding,
            Self::Grounding => Self::Hitbox,
            Self::Hitbox => Self::Off,
        }
    }
}

// Max health from `SInit` (the player, and per actor kind) — the denominator
// for every health bar. Starts empty (initialized at app build) and is
// replaced when `SInit` arrives; gameplay messages are buffered until the
// bootstrap has installed it.
#[derive(Resource, Default)]
pub struct MaxHealth {
    pub player: f32,
    pub actors: HashMap<String, f32>,
}

impl MaxHealth {
    #[must_use]
    pub fn actor(&self, kind: &str) -> f32 {
        self.actors
            .get(kind)
            .copied()
            .expect("actor kind sent by server is missing from SInit max health")
    }
}

#[must_use]
pub fn health_ratio(health: Health, max_health: f32) -> f32 {
    if max_health <= 0.0 {
        return 0.0;
    }
    (health.0 / max_health).clamp(0.0, 1.0)
}

#[cfg(test)]
#[path = "tests/resources.rs"]
mod tests;
