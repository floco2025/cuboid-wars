use anyhow::{Result, bail};
use serde::{Deserialize, Deserializer};

use super::validation::{validate_non_negative_finite, validate_positive_finite, validate_probability};

#[derive(Debug, Clone, Deserialize)]
pub struct ActorSettingsConfig {
    pub spawn_warning_secs: f32,
    pub threat_memory_secs: f32,
}

impl ActorSettingsConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.spawn_warning_secs, &format!("{path}.spawn_warning_secs"))?;
        validate_non_negative_finite(self.threat_memory_secs, &format!("{path}.threat_memory_secs"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorKindServerConfig {
    #[serde(deserialize_with = "deserialize_optional_f32")]
    pub respawn_delay_secs: Option<f32>,
    pub vision_range: f32,
    pub roam_steps: usize,
    pub combat: ActorCombatConfig,
}

fn deserialize_optional_f32<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<f32>::deserialize(deserializer)
}

impl ActorKindServerConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        if let Some(delay_secs) = self.respawn_delay_secs {
            validate_non_negative_finite(delay_secs, &format!("{path}.respawn_delay_secs"))?;
        }
        validate_positive_finite(self.vision_range, &format!("{path}.vision_range"))?;
        if self.roam_steps == 0 {
            bail!("{path}.roam_steps must be at least 1");
        }
        self.combat.validate(&format!("{path}.combat"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorCombatConfig {
    pub armor: f32,
    pub attack: ActorAttackConfig,
    pub death_explosion: ExplosionDamageConfig,
    #[serde(default)]
    pub score_reward_on_kill: i32,
}

impl ActorCombatConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_probability(self.armor, &format!("{path}.armor"))?;
        self.attack.validate(&format!("{path}.attack"))?;
        self.death_explosion.validate(&format!("{path}.death_explosion"))
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActorAttackConfig {
    Contact {
        trigger_gap: f32,
    },
    Beam {
        range: f32,
        duration_secs: f32,
        cooldown_secs: f32,
        damage_per_second: f32,
    },
}

impl ActorAttackConfig {
    #[must_use]
    pub const fn contact_trigger_gap(self) -> Option<f32> {
        match self {
            Self::Contact { trigger_gap } => Some(trigger_gap),
            Self::Beam { .. } => None,
        }
    }

    #[must_use]
    pub const fn beam(self) -> Option<ActorBeamAttackConfig> {
        match self {
            Self::Contact { .. } => None,
            Self::Beam {
                range,
                duration_secs,
                cooldown_secs,
                damage_per_second,
            } => Some(ActorBeamAttackConfig {
                range,
                duration_secs,
                cooldown_secs,
                damage_per_second,
            }),
        }
    }

    fn validate(self, path: &str) -> Result<()> {
        match self {
            Self::Contact { trigger_gap } => validate_non_negative_finite(trigger_gap, &format!("{path}.trigger_gap")),
            Self::Beam {
                range,
                duration_secs,
                cooldown_secs,
                damage_per_second,
            } => {
                validate_positive_finite(range, &format!("{path}.range"))?;
                validate_positive_finite(duration_secs, &format!("{path}.duration_secs"))?;
                validate_non_negative_finite(cooldown_secs, &format!("{path}.cooldown_secs"))?;
                validate_positive_finite(damage_per_second, &format!("{path}.damage_per_second"))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActorBeamAttackConfig {
    pub range: f32,
    pub duration_secs: f32,
    pub cooldown_secs: f32,
    pub damage_per_second: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ExplosionDamageConfig {
    pub radius: f32,
    pub max_damage: f32,
}

impl ExplosionDamageConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.radius, &format!("{path}.radius"))?;
        validate_non_negative_finite(self.max_damage, &format!("{path}.max_damage"))
    }
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
            .combat
            .attack
            .beam()
            .expect("zapper should have beam attack");
        assert_eq!(zapper.duration_secs, 2.0);
        assert_eq!(zapper.cooldown_secs, 5.0);
        assert_eq!(
            config.expect_actor("mine").combat.attack.contact_trigger_gap(),
            Some(0.4)
        );
        assert_eq!(
            config.expect_actor("sentry").combat.attack.contact_trigger_gap(),
            Some(0.8)
        );
    }

    #[test]
    fn actor_kind_rejects_zero_roam_steps() {
        let mut config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let actor = config.actors.get_mut("mine").expect("mine config");
        actor.roam_steps = 0;
        let err = actor.validate("actors.mine").expect_err("zero roam steps must fail");
        assert!(err.to_string().contains("roam_steps"));
    }

    #[test]
    fn beam_attack_rejects_non_positive_duration() {
        let attack = ActorAttackConfig::Beam {
            range: 15.0,
            duration_secs: 0.0,
            cooldown_secs: 5.0,
            damage_per_second: 75.0,
        };
        attack
            .validate("actors.test.combat.attack")
            .expect_err("zero duration must fail");
    }

    #[test]
    fn actor_kind_requires_explicit_respawn_setting() {
        let mut value = json!({
            "vision_range": 10.0,
            "roam_steps": 2,
            "combat": {
                "armor": 0.0,
                "attack": { "type": "contact", "trigger_gap": 0.1 },
                "death_explosion": { "radius": 1.0, "max_damage": 1.0 }
            }
        });

        let err = serde_json::from_value::<ActorKindServerConfig>(value.clone())
            .expect_err("respawn_delay_secs must be explicit");

        assert!(err.to_string().contains("respawn_delay_secs"));

        value["respawn_delay_secs"] = serde_json::Value::Null;
        let actor =
            serde_json::from_value::<ActorKindServerConfig>(value).expect("null should explicitly disable respawning");
        assert_eq!(actor.respawn_delay_secs, None);
    }
}
