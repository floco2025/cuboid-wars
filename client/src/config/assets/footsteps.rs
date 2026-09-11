use std::collections::HashMap;

use anyhow::{Result, ensure};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct FootstepSounds {
    pub volume_db: f32,
    pub ladder_volume_db: f32,
    pub default: String,
    pub sets: HashMap<String, FootstepSet>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FootstepSet {
    pub volume_db: f32,
    pub samples: Vec<String>,
    pub accent: Option<FootstepAccent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FootstepAccent {
    pub samples: Vec<String>,
    pub volume_db: f32,
}

impl FootstepSounds {
    pub(super) fn validate(&self) -> Result<()> {
        validate_volume(self.volume_db, "footsteps.volume_db")?;
        validate_volume(self.ladder_volume_db, "footsteps.ladder_volume_db")?;
        for (name, set) in &self.sets {
            ensure!(!name.trim().is_empty(), "footsteps.sets contains an empty name");
            validate_samples(&set.samples, name)?;
            validate_volume(set.volume_db, &format!("footsteps.sets.{name}.volume_db"))?;
            if let Some(accent) = &set.accent {
                validate_samples(&accent.samples, &format!("{name}.accent"))?;
                validate_volume(accent.volume_db, &format!("footsteps.sets.{name}.accent.volume_db"))?;
            }
        }
        self.validate_binding(&self.default, "footsteps.default")
    }

    pub(super) fn validate_binding(&self, sound: &str, path: &str) -> Result<()> {
        ensure!(
            self.sets.contains_key(sound),
            "{path} names unknown footstep set {sound}"
        );
        Ok(())
    }

    pub fn resolve(&self, binding: Option<&str>) -> &FootstepSet {
        self.sets
            .get(binding.unwrap_or(&self.default))
            .expect("footstep set missing")
    }
}

fn validate_volume(value: f32, path: &str) -> Result<()> {
    ensure!(
        value.is_finite() && 10.0_f32.powf(value / 20.0).is_finite(),
        "{path} must produce a finite gain"
    );
    Ok(())
}

fn validate_samples(samples: &[String], name: &str) -> Result<()> {
    ensure!(
        !samples.is_empty() && samples.iter().all(|s| !s.trim().is_empty()),
        "footsteps.sets.{name} must contain sound paths"
    );
    Ok(())
}

#[cfg(test)]
#[path = "tests/footsteps.rs"]
mod tests;
