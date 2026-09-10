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
        ensure!(self.server_hz >= 1, "network.server_hz must be positive");
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

    #[must_use]
    pub const fn update_cadence(&self) -> UpdateCadence {
        UpdateCadence::new(self.update_hz, self.server_hz)
    }

    #[must_use]
    pub const fn snapshot_cadence(&self) -> UpdateCadence {
        UpdateCadence::new(self.snapshot_hz, self.server_hz)
    }
}

// Spreads `rate` sends evenly over each second of `server_hz` ticks; the first call is always due.
#[derive(Debug, Clone, Copy, Default)]
pub struct UpdateCadence {
    rate: u32,
    server_hz: u32,
    started: bool,
    phase: u64,
}

impl UpdateCadence {
    #[must_use]
    pub const fn new(rate: u32, server_hz: u32) -> Self {
        Self {
            rate,
            server_hz,
            started: false,
            phase: 0,
        }
    }

    pub fn ready(&mut self) -> bool {
        if !self.started {
            self.started = true;
            return true;
        }
        self.phase += u64::from(self.rate);
        if self.phase < u64::from(self.server_hz) {
            return false;
        }
        self.phase -= u64::from(self.server_hz);
        true
    }
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
