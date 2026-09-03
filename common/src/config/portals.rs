use anyhow::{Result, bail};
use bincode::{Decode, Encode};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
pub struct PortalsConfig {
    pub range: f32,
}

impl PortalsConfig {
    pub fn validate(&self, path: &str) -> Result<()> {
        if !(self.range.is_finite() && self.range > 0.0) {
            bail!("{path}.range must be positive, got {}", self.range);
        }
        Ok(())
    }
}
