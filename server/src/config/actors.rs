use std::collections::HashMap;

use crate::schedule::ticks_from_secs;
use anyhow::{Result, bail};
use common::config::ActorGameplayConfig;
use serde::Deserialize;

use super::validation::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Deserialize)]
pub struct ActorsConfig {
    pub settings: ActorSettingsConfig,
    pub kinds: HashMap<String, ActorKindServerConfig>,
}

impl ActorsConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        self.settings.validate(&format!("{path}.settings"))?;
        if self.kinds.is_empty() {
            bail!("{path}.kinds must define at least one kind");
        }
        for (kind, actor) in &self.kinds {
            if kind.is_empty() {
                bail!("{path}.kinds contains an empty kind name");
            }
            actor.validate(&format!("{path}.kinds.{kind}"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorSettingsConfig {
    pub spawn_warning_secs: f32,
    pub threat_memory_secs: f32,
}

impl ActorSettingsConfig {
    #[must_use]
    pub fn spawn_warning_ticks(&self, server_hz: u32) -> u32 {
        ticks_from_secs(self.spawn_warning_secs, server_hz)
    }

    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.spawn_warning_secs, &format!("{path}.spawn_warning_secs"))?;
        validate_non_negative_finite(self.threat_memory_secs, &format!("{path}.threat_memory_secs"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorKindServerConfig {
    #[serde(flatten)]
    pub character: ActorGameplayConfig,
    pub vision_range: f32,
    pub roam_steps: usize,
    pub attack: ActorAttackConfig,
}

impl ActorKindServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        self.character.validate(path)?;
        if !self.character.immovable && self.roam_steps == 0 {
            bail!("{path}.roam_steps must be at least 1 for mobile actors");
        }
        validate_positive_finite(self.vision_range, &format!("{path}.vision_range"))?;
        self.attack.validate(&format!("{path}.attack"))
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActorAttackConfig {
    Contact(ContactAttackConfig),
    Beam(ActorBeamAttackConfig),
    ContactBeam(ContactBeamAttackConfig),
}

impl ActorAttackConfig {
    #[must_use]
    pub const fn contact_trigger_gap(self) -> Option<f32> {
        match self {
            Self::Contact(contact) | Self::ContactBeam(ContactBeamAttackConfig { contact, .. }) => {
                Some(contact.trigger_gap)
            }
            Self::Beam(_) => None,
        }
    }

    #[must_use]
    pub const fn beam(self) -> Option<ActorBeamAttackConfig> {
        match self {
            Self::Contact(_) => None,
            Self::Beam(beam) | Self::ContactBeam(ContactBeamAttackConfig { beam, .. }) => Some(beam),
        }
    }

    pub const fn beam_range(self) -> Option<f32> {
        match self.beam() {
            Some(beam) => Some(beam.range),
            None => None,
        }
    }

    fn validate(self, path: &str) -> Result<()> {
        match self {
            Self::Contact(contact) => contact.validate(path),
            Self::Beam(beam) => beam.validate(path),
            Self::ContactBeam(ContactBeamAttackConfig { contact, beam }) => {
                contact.validate(path)?;
                beam.validate(path)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct ContactAttackConfig {
    pub trigger_gap: f32,
}

impl ContactAttackConfig {
    fn validate(self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.trigger_gap, &format!("{path}.trigger_gap"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct ActorBeamAttackConfig {
    pub range: f32,
    pub duration_secs: f32,
    pub cooldown_secs: f32,
}

impl ActorBeamAttackConfig {
    fn validate(self, path: &str) -> Result<()> {
        validate_positive_finite(self.range, &format!("{path}.range"))?;
        validate_positive_finite(self.duration_secs, &format!("{path}.duration_secs"))?;
        validate_non_negative_finite(self.cooldown_secs, &format!("{path}.cooldown_secs"))
    }
}

// Both attacks in one kind: contact detonation plus a beam fired on the move.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct ContactBeamAttackConfig {
    #[serde(flatten)]
    pub contact: ContactAttackConfig,
    #[serde(flatten)]
    pub beam: ActorBeamAttackConfig,
}

#[cfg(test)]
#[path = "tests/actors.rs"]
mod tests;
