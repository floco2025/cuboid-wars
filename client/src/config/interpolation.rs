use anyhow::Result;
use common::config::NetworkConfig;
use serde::Deserialize;

use super::settings::validate_positive_finite;

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct InterpolationConfig {
    pub buffer_intervals: f32,
}

impl InterpolationConfig {
    pub(super) fn validate(&self) -> Result<()> {
        validate_positive_finite(self.buffer_intervals, "interpolation.buffer_intervals")
    }

    #[must_use]
    pub fn delay_ticks(&self, network: &NetworkConfig) -> f64 {
        f64::from(self.buffer_intervals) * f64::from(network.server_hz) / f64::from(network.update_hz)
    }
}
