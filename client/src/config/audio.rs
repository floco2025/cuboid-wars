use anyhow::{Result, bail};
use serde::Deserialize;

use super::settings::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub spatial_distance_scale: f32,
    pub explosion_gain_multiplier: f32,
    pub projectile_impacts: ProjectileImpactAudioConfig,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            spatial_distance_scale: 0.1,
            explosion_gain_multiplier: 2.0,
            projectile_impacts: ProjectileImpactAudioConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct ProjectileImpactAudioConfig {
    pub wall_bounce_gain_multiplier: f32,
    pub min_bounce_speed_meters_per_second: f32,
    pub max_bounce_sounds_per_second: f32,
    pub rate_limit_preemption_loudness_ratio: f32,
}

impl Default for ProjectileImpactAudioConfig {
    fn default() -> Self {
        Self {
            wall_bounce_gain_multiplier: 0.2,
            min_bounce_speed_meters_per_second: 10.0,
            max_bounce_sounds_per_second: 30.0,
            rate_limit_preemption_loudness_ratio: 2.0,
        }
    }
}

impl AudioConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_positive_finite(self.spatial_distance_scale, "audio.spatial_distance_scale")?;
        validate_non_negative_finite(self.explosion_gain_multiplier, "audio.explosion_gain_multiplier")?;
        self.projectile_impacts.validate()
    }
}

impl ProjectileImpactAudioConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_non_negative_finite(
            self.wall_bounce_gain_multiplier,
            "audio.projectile_impacts.wall_bounce_gain_multiplier",
        )?;
        validate_non_negative_finite(
            self.min_bounce_speed_meters_per_second,
            "audio.projectile_impacts.min_bounce_speed_meters_per_second",
        )?;
        validate_positive_finite(
            self.max_bounce_sounds_per_second,
            "audio.projectile_impacts.max_bounce_sounds_per_second",
        )?;
        if !(self.rate_limit_preemption_loudness_ratio.is_finite() && self.rate_limit_preemption_loudness_ratio >= 1.0)
        {
            bail!("audio.projectile_impacts.rate_limit_preemption_loudness_ratio must be finite and at least 1.0");
        }
        Ok(())
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

    #[test]
    fn audio_config_rejects_preemption_ratio_below_one() {
        let mut config = AudioConfig::default();
        config.projectile_impacts.rate_limit_preemption_loudness_ratio = 0.9;
        let error = config.validate().expect_err("preemption ratio below one should fail");
        assert!(error.to_string().contains("rate_limit_preemption_loudness_ratio"));
    }
}
