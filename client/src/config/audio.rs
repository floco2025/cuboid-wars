use anyhow::{Result, ensure};
use serde::Deserialize;

use super::settings::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
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
#[serde(default)]
pub struct BumpAudioConfig {
    pub min_run_up: f32,
    pub full_run_up: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            spatial_distance_scale: 0.1,
            explosion_gain: 2.0,
            rain_volume: 1.0,
            bump: BumpAudioConfig::default(),
        }
    }
}

impl Default for BumpAudioConfig {
    fn default() -> Self {
        Self {
            min_run_up: 2.0,
            full_run_up: 6.0,
        }
    }
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
mod tests {
    use super::*;

    #[test]
    fn default_audio_config_is_valid() {
        AudioConfig::default()
            .validate()
            .expect("default audio config should validate");
    }

    #[test]
    fn bump_volume_is_silent_below_the_threshold_and_ramps_to_full() {
        let bump = BumpAudioConfig {
            min_run_up: 2.0,
            full_run_up: 6.0,
        };
        assert_eq!(bump.volume_for(1.9), None);
        assert_eq!(bump.volume_for(2.0), Some(0.0));
        assert!((bump.volume_for(4.0).expect("mid run-up silent") - 0.5).abs() < 1e-6);
        assert_eq!(bump.volume_for(6.0), Some(1.0));
        assert_eq!(bump.volume_for(20.0), Some(1.0));
    }

    #[test]
    fn bump_config_rejects_a_full_run_up_at_or_below_the_threshold() {
        let config = AudioConfig {
            bump: BumpAudioConfig {
                min_run_up: 3.0,
                full_run_up: 3.0,
            },
            ..AudioConfig::default()
        };
        let error = config.validate().expect_err("flat ramp accepted");
        assert!(error.to_string().contains("audio.bump.full_run_up"), "{error}");
    }

    #[test]
    fn audio_config_rejects_zero_spatial_distance_scale() {
        let config = AudioConfig {
            spatial_distance_scale: 0.0,
            ..AudioConfig::default()
        };
        let error = config.validate().expect_err("zero spatial distance scale should fail");
        assert!(error.to_string().contains("spatial_distance_scale"));
    }
}
