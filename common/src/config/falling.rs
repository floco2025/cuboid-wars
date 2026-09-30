use anyhow::{Result, bail};
use serde::Deserialize;

use super::validation::validate_non_negative_finite;
use crate::constants::CHARACTER_TERMINAL_VELOCITY;

// Landing thresholds as normal-gravity drops in metres: no damage up to
// `safe_distance`, full health at `lethal_distance`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallDamageConfig {
    pub safe_distance: f32,
    pub lethal_distance: f32,
}

impl FallDamageConfig {
    pub fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.safe_distance, &format!("{path}.safe_distance"))?;
        validate_non_negative_finite(self.lethal_distance, &format!("{path}.lethal_distance"))?;
        if self.safe_distance >= self.lethal_distance {
            bail!(
                "{path}.safe_distance ({}) must be < lethal_distance ({})",
                self.safe_distance,
                self.lethal_distance
            );
        }
        Ok(())
    }

    // A lethal distance no ordinary fall reaches before terminal velocity is
    // a warning the server prints; validation itself stays silent.
    #[must_use]
    pub fn terminal_velocity_warning(&self, path: &str, normal_gravity: f32) -> Option<String> {
        let lethal_speed = (2.0 * f64::from(normal_gravity) * f64::from(self.lethal_distance)).sqrt();
        if lethal_speed <= f64::from(CHARACTER_TERMINAL_VELOCITY) {
            return None;
        }
        Some(format!(
            "{path}.lethal_distance ({} m) requires an impact speed of {lethal_speed:.2} m/s at gravity {normal_gravity} m/s², exceeding terminal velocity ({CHARACTER_TERMINAL_VELOCITY} m/s); ordinary falls cannot be lethal at full health",
            self.lethal_distance
        ))
    }
}

#[cfg(test)]
#[path = "tests/falling.rs"]
mod tests;
