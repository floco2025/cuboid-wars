use anyhow::Result;
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::validation::validate_positive_finite;

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
pub struct PortalsConfig {
    pub range: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Encode, Decode, Deserialize)]
pub struct PortalShotSettings {
    pub barriers_block: bool,
    pub light_bridges_block: bool,
}

impl PortalsConfig {
    pub fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.range, &format!("{path}.range"))
    }
}
