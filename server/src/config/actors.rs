use std::collections::HashMap;

use anyhow::{Result, bail};
use common::{config::ActorGameplayConfig, protocol::ticks_from_secs};
use serde::Deserialize;

use super::validation::{deserialize_required_option, validate_non_negative_finite, validate_positive_finite};

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
    pub fn spawn_warning_ticks(&self) -> u32 {
        ticks_from_secs(self.spawn_warning_secs)
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
    #[serde(deserialize_with = "deserialize_required_option")]
    pub respawn_secs: Option<f32>,
    pub vision_range: f32,
    pub roam_steps: usize,
    pub attack: ActorAttackConfig,
}

impl ActorKindServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        self.character.validate(path)?;
        if let Some(delay_secs) = self.respawn_secs {
            validate_non_negative_finite(delay_secs, &format!("{path}.respawn_secs"))?;
        }
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
    ContinuousBeam(ContinuousBeamAttackConfig),
    ContactBeam(ContactBeamAttackConfig),
}

impl ActorAttackConfig {
    #[must_use]
    pub const fn contact_trigger_gap(self) -> Option<f32> {
        match self {
            Self::Contact(contact) | Self::ContactBeam(ContactBeamAttackConfig { contact, .. }) => {
                Some(contact.trigger_gap)
            }
            Self::Beam(_) | Self::ContinuousBeam(_) => None,
        }
    }

    #[must_use]
    pub const fn beam(self) -> Option<ActorBeamAttackConfig> {
        match self {
            Self::Contact(_) | Self::ContinuousBeam(_) => None,
            Self::Beam(beam) | Self::ContactBeam(ContactBeamAttackConfig { beam, .. }) => Some(beam),
        }
    }

    pub const fn beam_range(self) -> Option<f32> {
        match self {
            Self::ContinuousBeam(beam) => Some(beam.range),
            _ => match self.beam() {
                Some(beam) => Some(beam.range),
                None => None,
            },
        }
    }

    fn validate(self, path: &str) -> Result<()> {
        match self {
            Self::Contact(contact) => contact.validate(path),
            Self::Beam(beam) => beam.validate(path),
            Self::ContinuousBeam(beam) => validate_positive_finite(beam.range, &format!("{path}.range")),
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

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ContinuousBeamAttackConfig {
    pub range: f32,
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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn beam_attack_rejects_non_positive_duration() {
        let attack = ActorAttackConfig::Beam(ActorBeamAttackConfig {
            range: 15.0,
            duration_secs: 0.0,
            cooldown_secs: 5.0,
        });
        attack
            .validate("actors.test.attack")
            .expect_err("zero duration must fail");
    }

    #[test]
    fn actor_kind_requires_explicit_respawn_setting() {
        let mut value = json!({
            "collider": {
                "width": 1.0,
                "height": 1.0,
                "depth": 1.0,
                "y_offset": 0.1,
                "y_offset_anchor": "bottom"
            },
            "support_probe": { "width": 0.2, "depth": 0.2 },
            "eye_height": 1.0,
            "can_use_ladders": false,
            "immovable": false,
            "vision_range": 10.0,
            "roam_steps": 2,
            "attack": { "type": "contact", "trigger_gap": 0.1 }
        });

        let err =
            serde_json::from_value::<ActorKindServerConfig>(value.clone()).expect_err("respawn_secs must be explicit");

        assert!(err.to_string().contains("respawn_secs"));

        value["respawn_secs"] = serde_json::Value::Null;
        let actor =
            serde_json::from_value::<ActorKindServerConfig>(value).expect("null should explicitly disable respawning");
        assert_eq!(actor.respawn_secs, None);
    }
}
