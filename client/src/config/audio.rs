use anyhow::{Result, ensure};
use serde::Deserialize;

use super::settings::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct AudioConfig {
    pub spatial_distance_scale: f32,
    pub explosion_gain: f32,
    // Rain-loop gain at full intensity.
    pub rain_volume: f32,
    pub bump: BumpAudioConfig,
}

// The local player's wall and player bump, scaled by run-up: the distance
// (m) the body travelled since it last stood still or hit something.
// Acceleration is instant, so speed says nothing about a hit; the run-up
// tells a dash across the room from a hop at a wall from close by. Silent
// below `min_run_up`, full volume from `full_run_up` up, linear between.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BumpAudioConfig {
    pub min_run_up: f32,
    pub full_run_up: f32,
}

impl AudioConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_positive_finite(self.spatial_distance_scale, "audio.spatial_distance_scale")?;
        validate_non_negative_finite(self.explosion_gain, "audio.explosion_gain")?;
        validate_non_negative_finite(self.rain_volume, "audio.rain_volume")?;
        self.bump.validate()
    }
}

impl BumpAudioConfig {
    fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.min_run_up, "audio.bump.min_run_up")?;
        validate_positive_finite(self.full_run_up, "audio.bump.full_run_up")?;
        ensure!(
            self.full_run_up > self.min_run_up,
            "audio.bump.full_run_up must be greater than audio.bump.min_run_up"
        );
        Ok(())
    }

    // Playback volume for a hit after `run_up` metres, `None` when it is too
    // soft to play.
    #[must_use]
    pub fn volume_for(&self, run_up: f32) -> Option<f32> {
        if run_up < self.min_run_up {
            return None;
        }
        let ramp = (run_up - self.min_run_up) / (self.full_run_up - self.min_run_up);
        Some(ramp.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
#[path = "tests/audio.rs"]
mod tests;
