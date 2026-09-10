use anyhow::{Result, bail};
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::validation::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
pub struct MissilesConfig {
    pub lock_range: f32,
    pub lock_assist_radius: f32,
    pub require_lock: bool,
    pub max_missiles: u32,
    pub turn_radius: f32,
    pub lifetime_secs: f32,
    pub launch_spread_degrees: f32,
    pub weave_strength: f32,
    pub proximity_fuse_distance: f32,
    pub stall_secs: f32,
}

impl MissilesConfig {
    pub fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.lock_range, &format!("{path}.lock_range"))?;
        validate_positive_finite(self.lock_assist_radius, &format!("{path}.lock_assist_radius"))?;
        validate_positive_finite(self.turn_radius, &format!("{path}.turn_radius"))?;
        validate_positive_finite(self.lifetime_secs, &format!("{path}.lifetime_secs"))?;
        if !(self.launch_spread_degrees.is_finite() && (0.0..=90.0).contains(&self.launch_spread_degrees)) {
            bail!(
                "{path}.launch_spread_degrees must be in [0, 90], got {}",
                self.launch_spread_degrees
            );
        }
        validate_non_negative_finite(self.weave_strength, &format!("{path}.weave_strength"))?;
        validate_non_negative_finite(self.proximity_fuse_distance, &format!("{path}.proximity_fuse_distance"))?;
        validate_positive_finite(self.stall_secs, &format!("{path}.stall_secs"))?;
        if self.max_missiles == 0 {
            bail!("{path}.max_missiles must be at least 1");
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/missiles.rs"]
mod tests;
