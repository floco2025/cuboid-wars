use anyhow::{Result, bail};
use bevy::prelude::Resource;
use serde::Deserialize;

use super::validation::validate_non_negative_finite;

#[derive(Resource, Debug, Clone, Copy, Deserialize)]
pub struct FallDamageConfig {
    pub safe_distance: f32,
    pub lethal_distance: f32,
}

impl FallDamageConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
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
}
