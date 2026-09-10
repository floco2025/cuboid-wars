use anyhow::{Result, bail};
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::validation::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
pub struct MissilesConfig {
    pub lock_range: f32,
    pub lock_assist_radius: f32,
    pub require_lock: bool,
    pub max_missiles: u32,
    pub turn_radius: f32,
    pub lifetime_secs: f32,
    pub launch_spread_degrees: f32,
    pub weave_strength: f32,
    pub proximity_fuse_distance: f32,
    pub stall_secs: f32,
}

impl MissilesConfig {
    pub fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.lock_range, &format!("{path}.lock_range"))?;
        validate_positive_finite(self.lock_assist_radius, &format!("{path}.lock_assist_radius"))?;
        validate_positive_finite(self.turn_radius, &format!("{path}.turn_radius"))?;
        validate_positive_finite(self.lifetime_secs, &format!("{path}.lifetime_secs"))?;
        if !(self.launch_spread_degrees.is_finite() && (0.0..=90.0).contains(&self.launch_spread_degrees)) {
            bail!(
                "{path}.launch_spread_degrees must be in [0, 90], got {}",
                self.launch_spread_degrees
            );
        }
        validate_non_negative_finite(self.weave_strength, &format!("{path}.weave_strength"))?;
        validate_non_negative_finite(self.proximity_fuse_distance, &format!("{path}.proximity_fuse_distance"))?;
        validate_positive_finite(self.stall_secs, &format!("{path}.stall_secs"))?;
        if self.max_missiles == 0 {
            bail!("{path}.max_missiles must be at least 1");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn missiles_config() -> MissilesConfig {
        MissilesConfig {
            lock_range: 60.0,
            lock_assist_radius: 1.2,
            require_lock: true,
            max_missiles: 3,
            turn_radius: 1.7,
            lifetime_secs: 10.0,
            launch_spread_degrees: 45.0,
            weave_strength: 0.1,
            proximity_fuse_distance: 1.0,
            stall_secs: 2.0,
        }
    }

    #[test]
    fn accepts_valid_values() {
        assert!(missiles_config().validate("missiles").is_ok());
    }

    #[test]
    fn rejects_zero_max_missiles() {
        let config = MissilesConfig {
            max_missiles: 0,
            ..missiles_config()
        };
        let err = config
            .validate("missiles")
            .expect_err("zero max_missiles passed validation");
        assert!(err.to_string().contains("max_missiles"));
    }

    #[test]
    fn rejects_out_of_range_flight_tuning_by_field() {
        let cases: [(&str, fn(&mut MissilesConfig)); 6] = [
            ("turn_radius", |config| config.turn_radius = 0.0),
            ("lifetime_secs", |config| config.lifetime_secs = -1.0),
            ("launch_spread_degrees", |config| config.launch_spread_degrees = 91.0),
            ("weave_strength", |config| config.weave_strength = -0.1),
            ("proximity_fuse_distance", |config| {
                config.proximity_fuse_distance = f32::NAN;
            }),
            ("stall_secs", |config| config.stall_secs = 0.0),
        ];
        for (field, break_it) in cases {
            let mut config = missiles_config();
            break_it(&mut config);
            let err = config
                .validate("missiles")
                .expect_err(&format!("{field} passed validation"))
                .to_string();
            assert!(err.contains(field), "{field} is not named in {err}");
        }
    }

    #[test]
    fn rejects_non_positive_lock_distance() {
        let config = MissilesConfig {
            lock_range: 0.0,
            ..missiles_config()
        };
        assert!(config.validate("missiles").is_err());
    }
}
