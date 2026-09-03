use anyhow::{Result, bail};
use serde::Deserialize;

use super::validation::{deserialize_required_option, validate_positive_finite};

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
mod tests {
    use super::*;

    fn ok_weather_cycle() -> WeatherCycleConfig {
        WeatherCycleConfig {
            min_clear_secs: 10.0,
            max_clear_secs: 20.0,
            min_rain_secs: 5.0,
            max_rain_secs: 8.0,
            ramp_in_secs: 2.0,
            fade_out_secs: 4.0,
        }
    }

    #[test]
    fn weather_cycle_accepts_valid_config() {
        ok_weather_cycle()
            .validate("weather_cycle")
            .expect("valid weather cycle should pass");
    }

    #[test]
    fn weather_cycle_rejects_min_clear_above_max() {
        let mut cycle = ok_weather_cycle();
        cycle.min_clear_secs = 30.0;
        let err = cycle
            .validate("weather_cycle")
            .expect_err("min_clear > max_clear must be rejected");
        assert!(err.to_string().contains("min_clear_secs"));
    }

    #[test]
    fn weather_cycle_rejects_min_rain_above_max() {
        let mut cycle = ok_weather_cycle();
        cycle.min_rain_secs = 30.0;
        let err = cycle
            .validate("weather_cycle")
            .expect_err("min_rain > max_rain must be rejected");
        assert!(err.to_string().contains("min_rain_secs"));
    }

    #[test]
    fn weather_cycle_rejects_non_positive_ramp() {
        let mut cycle = ok_weather_cycle();
        cycle.ramp_in_secs = 0.0;
        let err = cycle
            .validate("weather_cycle")
            .expect_err("zero ramp_in must be rejected");
        assert!(err.to_string().contains("ramp_in_secs"));
    }

    fn full_lighting_cycle() -> LightingCycleConfig {
        LightingCycleConfig {
            bright_secs: Some(240.0),
            dim_secs: Some(45.0),
            dark_secs: Some(120.0),
            bright_dim_secs: Some(20.0),
            dim_dark_secs: Some(20.0),
            bright_dark_secs: None,
        }
    }

    #[test]
    fn lighting_cycle_parses_and_validates() {
        let cycle: LightingCycleConfig = serde_json::from_str(
            r#"{"bright_secs": 240.0, "dim_secs": 45.0, "dark_secs": 120.0, "bright_dim_secs": 20.0, "dim_dark_secs": 20.0, "bright_dark_secs": null}"#,
        )
        .expect("lighting cycle should deserialize");
        cycle
            .validate("lighting_cycle")
            .expect("valid lighting cycle should pass");
    }

    #[test]
    fn lighting_cycle_rejects_non_positive_durations() {
        let ok = full_lighting_cycle();
        for (cycle, field) in [
            (
                LightingCycleConfig {
                    bright_secs: Some(0.0),
                    ..ok.clone()
                },
                "bright_secs",
            ),
            (
                LightingCycleConfig {
                    dim_secs: Some(-1.0),
                    ..ok.clone()
                },
                "dim_secs",
            ),
            (
                LightingCycleConfig {
                    bright_dim_secs: Some(0.0),
                    ..ok.clone()
                },
                "bright_dim_secs",
            ),
        ] {
            let err = cycle
                .validate("lighting_cycle")
                .expect_err("non-positive duration must be rejected");
            assert!(err.to_string().contains(field));
        }
    }

    #[test]
    fn lighting_cycle_accepts_two_stop_variants() {
        let bright_dim = LightingCycleConfig {
            dark_secs: None,
            dim_dark_secs: None,
            ..full_lighting_cycle()
        };
        bright_dim
            .validate("lighting_cycle")
            .expect("bright+dim cycle should pass");

        let dim_dark = LightingCycleConfig {
            bright_secs: None,
            bright_dim_secs: None,
            ..full_lighting_cycle()
        };
        dim_dark.validate("lighting_cycle").expect("dim+dark cycle should pass");

        let bright_dark = LightingCycleConfig {
            dim_secs: None,
            bright_dim_secs: None,
            dim_dark_secs: None,
            bright_dark_secs: Some(30.0),
            ..full_lighting_cycle()
        };
        bright_dark
            .validate("lighting_cycle")
            .expect("bright+dark cycle should pass");
    }

    #[test]
    fn lighting_cycle_rejects_single_stop() {
        let cycle = LightingCycleConfig {
            dim_secs: None,
            dark_secs: None,
            bright_dim_secs: None,
            dim_dark_secs: None,
            ..full_lighting_cycle()
        };
        let err = cycle
            .validate("lighting_cycle")
            .expect_err("single-stop cycle must be rejected");
        assert!(err.to_string().contains("at least two"));
    }

    #[test]
    fn lighting_cycle_rejects_missing_and_unused_fades() {
        let missing = LightingCycleConfig {
            bright_dim_secs: None,
            ..full_lighting_cycle()
        };
        let err = missing
            .validate("lighting_cycle")
            .expect_err("missing bright_dim fade must be rejected");
        assert!(err.to_string().contains("bright_dim_secs is required"));

        let unused = LightingCycleConfig {
            bright_dark_secs: Some(30.0),
            ..full_lighting_cycle()
        };
        let err = unused
            .validate("lighting_cycle")
            .expect_err("bright_dark fade with dim present must be rejected");
        assert!(err.to_string().contains("bright_dark_secs is not used"));

        let bright_dark_missing = LightingCycleConfig {
            dim_secs: None,
            bright_dim_secs: None,
            dim_dark_secs: None,
            bright_dark_secs: None,
            ..full_lighting_cycle()
        };
        let err = bright_dark_missing
            .validate("lighting_cycle")
            .expect_err("bright+dark cycle without its fade must be rejected");
        assert!(err.to_string().contains("bright_dark_secs is required"));
    }
}
