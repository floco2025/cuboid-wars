use anyhow::Result;
use serde::Deserialize;

use super::missiles::MissilesServerConfig;
use common::config::{PortalsConfig, ProjectilesConfig};

#[derive(Debug, Clone, Deserialize)]
pub struct WeaponsConfig {
    pub projectiles: ProjectilesConfig,
    pub missiles: MissilesServerConfig,
    pub portals: PortalsConfig,
}

impl WeaponsConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        self.projectiles.validate(&format!("{path}.projectiles"))?;
        self.missiles.validate(&format!("{path}.missiles"))?;
        self.portals.validate(&format!("{path}.portals"))
    }
}
