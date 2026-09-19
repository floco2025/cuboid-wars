use anyhow::{Result, bail};
use common::celestial::CelestialCycleSettings;
use serde::Deserialize;

use super::validation::validate_positive_finite;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CyclesConfig {
    pub weather: WeatherCycleConfig,
    pub celestial: CelestialCycleSettings,
}

impl CyclesConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        self.weather.validate(&format!("{path}.weather"))?;
        self.celestial.validate(&format!("{path}.celestial"))
    }
}

// Cadence of the automatic rain cycle: random clear stretch, linear cloud
// ramp, a random rainy stretch at full cover, linear cloud fade, repeat.
// The separate client precipitation envelope starts only after ramp-in and
// begins fading at the start of fade-out. A map opts in with
// `weather: "auto"`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherCycleConfig {
    pub min_clear_secs: f32,
    pub max_clear_secs: f32,
    pub min_rain_secs: f32,
    pub max_rain_secs: f32,
    pub ramp_in_secs: f32,
    pub fade_out_secs: f32,
}

impl WeatherCycleConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.min_clear_secs, &format!("{path}.min_clear_secs"))?;
        validate_positive_finite(self.max_clear_secs, &format!("{path}.max_clear_secs"))?;
        if self.min_clear_secs > self.max_clear_secs {
            bail!("{path}.min_clear_secs must be <= {path}.max_clear_secs");
        }
        validate_positive_finite(self.min_rain_secs, &format!("{path}.min_rain_secs"))?;
        validate_positive_finite(self.max_rain_secs, &format!("{path}.max_rain_secs"))?;
        if self.min_rain_secs > self.max_rain_secs {
            bail!("{path}.min_rain_secs must be <= {path}.max_rain_secs");
        }
        validate_positive_finite(self.ramp_in_secs, &format!("{path}.ramp_in_secs"))?;
        validate_positive_finite(self.fade_out_secs, &format!("{path}.fade_out_secs"))
    }
}

#[cfg(test)]
#[path = "tests/cycles.rs"]
mod tests;
