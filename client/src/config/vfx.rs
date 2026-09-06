use anyhow::Result;
use serde::Deserialize;

use super::settings::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct VfxConfig {
    pub pickup_emissive_brightness: f32,
    pub missile_exhaust: MissileExhaustVfxConfig,
}

impl Default for VfxConfig {
    fn default() -> Self {
        Self {
            pickup_emissive_brightness: 3.0,
            missile_exhaust: MissileExhaustVfxConfig::default(),
        }
    }
}

impl VfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.pickup_emissive_brightness, "vfx.pickup_emissive_brightness")?;
        self.missile_exhaust.validate()
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct MissileExhaustVfxConfig {
    pub particles_per_sec: f32,
    pub particle_size: f32,
    pub particle_lifetime_secs: f32,
    pub emissive_brightness: f32,
    // Speed of gas kicked backward out of the nozzle, m/s.
    pub back_speed: f32,
    // Upward drift of the cooling gas, m/s².
    pub rise_acceleration: f32,
    pub jitter: f32,
}

impl Default for MissileExhaustVfxConfig {
    fn default() -> Self {
        Self {
            particles_per_sec: 260.0,
            particle_size: 0.05,
            particle_lifetime_secs: 0.3,
            emissive_brightness: 25.0,
            back_speed: 2.5,
            rise_acceleration: 1.2,
            jitter: 0.25,
        }
    }
}

impl MissileExhaustVfxConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_non_negative_finite(self.particles_per_sec, "vfx.missile_exhaust.particles_per_sec")?;
        validate_positive_finite(self.particle_size, "vfx.missile_exhaust.particle_size")?;
        validate_positive_finite(
            self.particle_lifetime_secs,
            "vfx.missile_exhaust.particle_lifetime_secs",
        )?;
        validate_non_negative_finite(self.emissive_brightness, "vfx.missile_exhaust.emissive_brightness")?;
        validate_non_negative_finite(self.back_speed, "vfx.missile_exhaust.back_speed")?;
        validate_non_negative_finite(self.rise_acceleration, "vfx.missile_exhaust.rise_acceleration")?;
        validate_non_negative_finite(self.jitter, "vfx.missile_exhaust.jitter")?;
        Ok(())
    }
}
