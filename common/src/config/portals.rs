use anyhow::Result;
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::validation::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalSize {
    pub width: f32,
    pub height: f32,
}

impl PortalSize {
    pub fn half_width(self) -> f32 {
        self.width * 0.5
    }
    pub fn half_height(self) -> f32 {
        self.height * 0.5
    }
}

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalFunnelConfig {
    // Extra capture distance beyond each aperture edge, in metres. Zero disables assistance.
    pub capture_margin: f32,
}

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalsConfig {
    pub range: f32,
    pub size: PortalSize,
    pub funnel: PortalFunnelConfig,
}

impl PortalsConfig {
    pub fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.range, &format!("{path}.range"))?;
        validate_positive_finite(self.size.width, &format!("{path}.size.width"))?;
        validate_positive_finite(self.size.height, &format!("{path}.size.height"))?;
        validate_non_negative_finite(self.funnel.capture_margin, &format!("{path}.funnel.capture_margin"))
    }
}
