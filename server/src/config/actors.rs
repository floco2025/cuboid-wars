use std::collections::HashMap;

use anyhow::{Result, bail};
use common::{config::CharacterGameplayConfig, protocol::ticks_from_secs};
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
    pub character: CharacterGameplayConfig,
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
        validate_positive_finite(self.vision_range, &format!("{path}.vision_range"))?;
        if self.roam_steps == 0 {
            bail!("{path}.roam_steps must be at least 1");
        }
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
mod tests {
    use super::*;
    use crate::config::ServerGameplayConfig;
    use serde_json::json;

    #[test]
    fn default_config_loads_explicit_actor_attacks() {
        let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let zapper = config
            .expect_actor("zapper")
            .attack
            .beam()
            .expect("zapper should have beam attack");
        assert_eq!(zapper.duration_secs, 2.0);
        assert_eq!(zapper.cooldown_secs, 8.0);
        assert_eq!(config.expect_actor("mine").attack.contact_trigger_gap(), Some(0.4));
        assert_eq!(config.expect_actor("sentry").attack.contact_trigger_gap(), Some(0.8));
        let reaper = config.expect_actor("reaper").attack;
        assert_eq!(reaper.contact_trigger_gap(), Some(0.8));
        let reaper_beam = reaper.beam().expect("reaper should have beam attack");
        assert_eq!(reaper_beam.range, 25.0);
        assert_eq!(reaper_beam.duration_secs, 2.0);
        assert_eq!(reaper_beam.cooldown_secs, 5.0);
    }

    #[test]
    fn actor_kind_rejects_zero_roam_steps() {
        let mut config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let actor = config.actors.kinds.get_mut("mine").expect("mine config");
        actor.roam_steps = 0;
        let err = actor.validate("actors.mine").expect_err("zero roam steps must fail");
        assert!(err.to_string().contains("roam_steps"));
    }

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
