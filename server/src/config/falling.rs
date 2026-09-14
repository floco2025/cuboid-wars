use anyhow::{Result, bail};
use bevy::prelude::Resource;
use serde::Deserialize;

use super::validation::validate_non_negative_finite;
use common::constants::CHARACTER_TERMINAL_VELOCITY;

#[derive(Resource, Debug, Clone, Copy, Deserialize)]
pub struct FallDamageConfig {
    pub safe_distance: f32,
    pub lethal_distance: f32,
}

impl FallDamageConfig {
    pub(super) fn validate(&self, path: &str, normal_gravity: f32) -> Result<()> {
        validate_non_negative_finite(self.safe_distance, &format!("{path}.safe_distance"))?;
        validate_non_negative_finite(self.lethal_distance, &format!("{path}.lethal_distance"))?;
        if self.safe_distance >= self.lethal_distance {
            bail!(
                "{path}.safe_distance ({}) must be < lethal_distance ({})",
                self.safe_distance,
                self.lethal_distance
            );
        }
        if let Some(warning) = self.terminal_velocity_warning(path, normal_gravity) {
            // Configuration loads before the log plugin is installed.
            eprintln!("warning: {warning}");
        }
        Ok(())
    }

    fn terminal_velocity_warning(&self, path: &str, normal_gravity: f32) -> Option<String> {
        let lethal_speed = (2.0 * f64::from(normal_gravity) * f64::from(self.lethal_distance)).sqrt();
        if lethal_speed <= f64::from(CHARACTER_TERMINAL_VELOCITY) {
            return None;
        }
        Some(format!(
            "{path}.lethal_distance ({} m) requires an impact speed of {lethal_speed:.2} m/s at gravity {normal_gravity} m/s², exceeding terminal velocity ({CHARACTER_TERMINAL_VELOCITY} m/s); ordinary falls cannot kill a full-health player",
            self.lethal_distance
        ))
    }
}

#[cfg(test)]
#[path = "tests/falling.rs"]
mod tests;
