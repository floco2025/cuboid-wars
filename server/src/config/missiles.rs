use anyhow::{Result, bail};
use serde::Deserialize;

use common::config::MissilesConfig;

// Missile speed is selected per map; blast tuning lives in combat damage.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MissilesServerConfig {
    #[serde(flatten)]
    pub gameplay: MissilesConfig,
    pub missiles_per_pack: u32,
}

impl MissilesServerConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        self.gameplay.validate(path)?;
        if self.missiles_per_pack == 0 {
            bail!("{path}.missiles_per_pack must be at least 1");
        }
        Ok(())
    }
}
