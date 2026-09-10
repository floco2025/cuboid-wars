use std::time::Duration;

use anyhow::{Result, ensure};
use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};
use serde::Deserialize;

use crate::constants::TICK_HZ;

#[derive(Debug, Clone, Copy, Deserialize, Encode, Decode, Resource)]
#[serde(default)]
pub struct NetworkConfig {
    pub server_hz: u32,
    pub update_hz: u32,
    pub snapshot_hz: u32,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            server_hz: TICK_HZ,
            update_hz: TICK_HZ,
            snapshot_hz: 4,
        }
    }
}

impl NetworkConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=1_000_000_000).contains(&self.server_hz),
            "network.server_hz must be positive and its tick must last at least one nanosecond"
        );
        ensure!(
            (1..=self.server_hz).contains(&self.update_hz),
            "network.update_hz must be between 1 and network.server_hz ({})",
            self.server_hz
        );
        ensure!(
            (1..=self.server_hz).contains(&self.snapshot_hz),
            "network.snapshot_hz must be between 1 and network.server_hz ({})",
            self.server_hz
        );
        Ok(())
    }

    #[must_use]
    pub fn tick_duration(&self) -> Duration {
        Duration::from_nanos(1_000_000_000 / u64::from(self.server_hz))
    }

    #[must_use]
    pub fn tick_secs(&self) -> f32 {
        1.0 / self.server_hz as f32
    }
}

#[cfg(test)]
#[path = "replication_tests.rs"]
mod tests;
