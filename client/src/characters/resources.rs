use std::collections::HashMap;

use bevy::prelude::*;

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
