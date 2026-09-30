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
    // Extra capture distance beyond each aperture edge, in metres, at the moment of arrival.
    pub capture_margin: f32,
    // Further capture distance for every second of flight still to go, in
    // metres per second: a long fall is caught from wider off. Both zero
    // disables assistance.
    pub capture_growth: f32,
}

impl PortalFunnelConfig {
    pub fn assists(self) -> bool {
        self.capture_margin > 0.0 || self.capture_growth > 0.0
    }

    // The capture distance beyond the aperture for a body this long from arriving.
    pub fn margin_at(self, time_to_arrival: f32) -> f32 {
        self.capture_margin + self.capture_growth * time_to_arrival
    }
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
        validate_non_negative_finite(self.funnel.capture_margin, &format!("{path}.funnel.capture_margin"))?;
        validate_non_negative_finite(self.funnel.capture_growth, &format!("{path}.funnel.capture_growth"))
    }
}
