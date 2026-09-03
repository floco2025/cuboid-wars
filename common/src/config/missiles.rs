use anyhow::{Result, bail};
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::validation::validate_positive_finite;

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
pub struct MissilesConfig {
    pub lock_range: f32,
    pub lock_assist_radius: f32,
    pub require_lock: bool,
    pub max_missiles: u32,
}

impl MissilesConfig {
    pub fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.lock_range, &format!("{path}.lock_range"))?;
        validate_positive_finite(self.lock_assist_radius, &format!("{path}.lock_assist_radius"))?;
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
    fn rejects_non_positive_lock_distance() {
        let config = MissilesConfig {
            lock_range: 0.0,
            ..missiles_config()
        };
        assert!(config.validate("missiles").is_err());
    }
}
