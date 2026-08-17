use anyhow::Result;
use serde::Deserialize;

use super::settings::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub spatial_distance_scale: f32,
    pub explosion_gain_multiplier: f32,
    // Rain-loop gain at full intensity.
    pub rain_volume: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            spatial_distance_scale: 0.1,
            explosion_gain_multiplier: 2.0,
            rain_volume: 1.0,
        }
    }
}

impl AudioConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_positive_finite(self.spatial_distance_scale, "audio.spatial_distance_scale")?;
        validate_non_negative_finite(self.explosion_gain_multiplier, "audio.explosion_gain_multiplier")?;
        validate_non_negative_finite(self.rain_volume, "audio.rain_volume")
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
    fn audio_config_rejects_zero_spatial_distance_scale() {
        let config = AudioConfig {
            spatial_distance_scale: 0.0,
            ..AudioConfig::default()
        };
        let error = config.validate().expect_err("zero spatial distance scale should fail");
        assert!(error.to_string().contains("spatial_distance_scale"));
    }
}
