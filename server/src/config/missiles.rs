use anyhow::{Result, bail};
use serde::Deserialize;

use super::validation::{validate_non_negative_finite, validate_positive_finite};
use common::config::MissilesConfig;

// Missile speed is selected per map; blast tuning lives in combat damage.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MissilesServerConfig {
    #[serde(flatten)]
    pub gameplay: MissilesConfig,
    pub turn_radius: f32,
    pub lifetime_secs: f32,
    pub launch_spread_degrees: f32,
    pub weave_strength: f32,
    pub proximity_fuse_distance: f32,
    pub stall_secs: f32,
    pub missiles_per_pack: u32,
}

impl MissilesServerConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
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
        if self.missiles_per_pack == 0 {
            bail!("{path}.missiles_per_pack must be at least 1");
        }
        Ok(())
    }
}
