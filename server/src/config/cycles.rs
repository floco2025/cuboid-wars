use anyhow::{Result, bail};
use serde::Deserialize;

use super::validation::{deserialize_required_option, validate_positive_finite};

#[derive(Debug, Clone, Deserialize)]
pub struct CyclesConfig {
    pub weather: WeatherCycleConfig,
    pub lighting: LightingCycleConfig,
}

impl CyclesConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        self.weather.validate(&format!("{path}.weather"))?;
        self.lighting.validate(&format!("{path}.lighting"))
    }
}

// Cadence of the automatic rain cycle: random clear stretch, ramp in, a
// random rain stretch at full intensity, fade out, repeat. Global — maps
// opt in with `weather: "auto"`.
#[derive(Debug, Clone, Deserialize)]
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

// Cadence of the automatic light cycle: hold at each present stop, fading
// between them, down and back up. Any two or three of bright/dim/dark can
// be stops — an absent hold means the cycle skips that look. Global — maps
// opt in with `lighting: "auto"`.
#[derive(Debug, Clone, Deserialize)]
pub struct LightingCycleConfig {
    #[serde(deserialize_with = "deserialize_required_option")]
    pub bright_secs: Option<f32>,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub dim_secs: Option<f32>,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub dark_secs: Option<f32>,
    // Fade lengths between adjacent stops, used in both directions. Exactly
    // the fades matching the present stop pairs must be set;
    // `bright_dark_secs` is the direct fade when dim is not a stop.
    #[serde(deserialize_with = "deserialize_required_option")]
    pub bright_dim_secs: Option<f32>,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub dim_dark_secs: Option<f32>,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub bright_dark_secs: Option<f32>,
}

impl LightingCycleConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        for (value, name) in [
            (self.bright_secs, "bright_secs"),
            (self.dim_secs, "dim_secs"),
            (self.dark_secs, "dark_secs"),
            (self.bright_dim_secs, "bright_dim_secs"),
            (self.dim_dark_secs, "dim_dark_secs"),
            (self.bright_dark_secs, "bright_dark_secs"),
        ] {
            if let Some(value) = value {
                validate_positive_finite(value, &format!("{path}.{name}"))?;
            }
        }
        let stops = [self.bright_secs, self.dim_secs, self.dark_secs]
            .iter()
            .filter(|stop| stop.is_some())
            .count();
        if stops < 2 {
            bail!(
                "{path} needs at least two of bright_secs/dim_secs/dark_secs — a one-stop cycle is constant; use a concrete `lighting` mode instead"
            );
        }
        for (fade, name, needed) in [
            (
                self.bright_dim_secs,
                "bright_dim_secs",
                self.bright_secs.is_some() && self.dim_secs.is_some(),
            ),
            (
                self.dim_dark_secs,
                "dim_dark_secs",
                self.dim_secs.is_some() && self.dark_secs.is_some(),
            ),
            (
                self.bright_dark_secs,
                "bright_dark_secs",
                self.bright_secs.is_some() && self.dark_secs.is_some() && self.dim_secs.is_none(),
            ),
        ] {
            if needed && fade.is_none() {
                bail!("{path}.{name} is required for this cycle's stops");
            }
            if !needed && fade.is_some() {
                bail!("{path}.{name} is not used by this cycle's stops");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/cycles.rs"]
mod tests;
