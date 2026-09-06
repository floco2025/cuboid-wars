use std::collections::HashMap;

use anyhow::{Result, bail};
use serde::Deserialize;

use super::{
    actors::ActorKindServerConfig,
    validation::{
        deserialize_required_option, validate_covers_actor_kinds, validate_non_negative_finite,
        validate_positive_finite,
    },
};

// Every health and damage number in the game, consolidated for balancing:
// how much everything can take, how hard everything hits.
#[derive(Debug, Clone, Deserialize)]
pub struct CombatConfig {
    pub health: HealthConfig,
    pub damage: DamageConfig,
}

impl CombatConfig {
    pub(super) fn validate(&self, actors: &HashMap<String, ActorKindServerConfig>) -> Result<()> {
        self.health.validate(actors)?;
        self.damage.validate(actors)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthConfig {
    pub player: PlayerHealthConfig,
    pub actors: HashMap<String, ActorHealthConfig>,
}

impl HealthConfig {
    #[must_use]
    pub fn expect_actor(&self, kind: &str) -> &ActorHealthConfig {
        self.actors
            .get(kind)
            .expect("actor kind missing from combat.health.actors")
    }

    fn validate(&self, actors: &HashMap<String, ActorKindServerConfig>) -> Result<()> {
        self.player.validate("combat.health.player")?;
        validate_covers_actor_kinds(self.actors.keys(), actors, "combat.health.actors")?;
        for (kind, health) in &self.actors {
            health.validate(&format!("combat.health.actors.{kind}"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PlayerHealthConfig {
    pub max: f32,
    // Regeneration as a fraction of `max` per second, so toughness is tuned
    // by `max` alone and the regen race scales with it.
    pub regen_rate: f32,
    // Fraction of `max` restored by one Health Potion pickup (1.0 = full
    // heal). Instant — no duration.
    pub potion_heal: f32,
}

impl PlayerHealthConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.max, &format!("{path}.max"))?;
        validate_non_negative_finite(self.regen_rate, &format!("{path}.regen_rate"))?;
        if !(self.potion_heal > 0.0 && self.potion_heal <= 1.0) {
            bail!("{path}.potion_heal must be in (0.0, 1.0], got {}", self.potion_heal);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ActorHealthConfig {
    pub max: f32,
    // A fraction of `max` per second, like the player's.
    pub regen_rate: f32,
}

impl ActorHealthConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.max, &format!("{path}.max"))?;
        validate_non_negative_finite(self.regen_rate, &format!("{path}.regen_rate"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DamageConfig {
    // Players only — actors never take fall damage.
    pub player_fall: FallDamageConfig,
    // One raw number per hit; players and actors take it alike.
    pub projectile: f32,
    pub missile_blast: BlastConfig,
    // Blast dealt by a dying player — standing next to your victim is a
    // mistake.
    pub player_blast: BlastConfig,
    pub actors: HashMap<String, ActorDamageConfig>,
}

impl DamageConfig {
    #[must_use]
    pub fn expect_actor(&self, kind: &str) -> &ActorDamageConfig {
        self.actors
            .get(kind)
            .expect("actor kind missing from combat.damage.actors")
    }

    fn validate(&self, actors: &HashMap<String, ActorKindServerConfig>) -> Result<()> {
        self.player_fall.validate("combat.damage.player_fall")?;
        validate_non_negative_finite(self.projectile, "combat.damage.projectile")?;
        self.missile_blast.validate("combat.damage.missile_blast")?;
        self.player_blast.validate("combat.damage.player_blast")?;
        validate_covers_actor_kinds(self.actors.keys(), actors, "combat.damage.actors")?;
        for (kind, actor) in actors {
            let damage = self.expect_actor(kind);
            let path = format!("combat.damage.actors.{kind}");
            damage.death_blast.validate(&format!("{path}.death_blast"))?;
            match (damage.beam_dps, actor.attack.beam()) {
                (Some(dps), Some(_)) => {
                    validate_positive_finite(dps, &format!("{path}.beam_dps"))?;
                }
                (None, None) => {}
                (Some(_), None) => {
                    bail!("{path}.beam_dps is set but actors.{kind}.attack fires no beam");
                }
                (None, Some(_)) => {
                    bail!("{path}.beam_dps is required because actors.{kind}.attack fires a beam");
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ActorDamageConfig {
    // Present exactly when the kind's `attack` fires a beam.
    #[serde(deserialize_with = "deserialize_required_option")]
    pub beam_dps: Option<f32>,
    pub death_blast: BlastConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FallDamageConfig {
    // Below this fall distance (meters), landing does no damage.
    pub safe_distance: f32,
    // At this fall distance, landing deals `max_health` damage (lethal).
    // Damage lerps linearly between the two endpoints and clamps past
    // `lethal_distance`.
    pub lethal_distance: f32,
}

impl FallDamageConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.safe_distance, &format!("{path}.safe_distance"))?;
        validate_non_negative_finite(self.lethal_distance, &format!("{path}.lethal_distance"))?;
        if self.safe_distance >= self.lethal_distance {
            bail!(
                "{path}.safe_distance ({}) must be < lethal_distance ({})",
                self.safe_distance,
                self.lethal_distance
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BlastConfig {
    pub radius: f32,
    pub max_damage: f32,
}

impl BlastConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.radius, &format!("{path}.radius"))?;
        validate_non_negative_finite(self.max_damage, &format!("{path}.max_damage"))
    }
}

#[cfg(test)]
mod tests {
    use crate::config::ServerGameplayConfig;

    fn config() -> ServerGameplayConfig {
        ServerGameplayConfig::load_default().expect("default server gameplay config should load")
    }

    #[test]
    fn health_rejects_missing_actor_kind() {
        let mut config = config();
        config.combat.health.actors.remove("mine");
        let err = config
            .combat
            .validate(&config.actors.kinds)
            .expect_err("missing kind must fail");
        assert!(err.to_string().contains("combat.health.actors"));
        assert!(err.to_string().contains("mine"));
    }

    #[test]
    fn damage_rejects_unknown_actor_kind() {
        let mut config = config();
        let zapper = *config.combat.damage.expect_actor("zapper");
        config.combat.damage.actors.insert("banana".to_owned(), zapper);
        let err = config
            .combat
            .validate(&config.actors.kinds)
            .expect_err("unknown kind must fail");
        assert!(err.to_string().contains("combat.damage.actors"));
        assert!(err.to_string().contains("banana"));
    }

    #[test]
    fn damage_rejects_beam_dps_on_contact_kind() {
        let mut config = config();
        config
            .combat
            .damage
            .actors
            .get_mut("mine")
            .expect("mine damage config")
            .beam_dps = Some(1.0);
        let err = config
            .combat
            .validate(&config.actors.kinds)
            .expect_err("beam dps on a contact kind must fail");
        assert!(err.to_string().contains("fires no beam"));
    }

    #[test]
    fn damage_requires_beam_dps_on_beam_kind() {
        let mut config = config();
        config
            .combat
            .damage
            .actors
            .get_mut("zapper")
            .expect("zapper damage config")
            .beam_dps = None;
        let err = config
            .combat
            .validate(&config.actors.kinds)
            .expect_err("missing beam dps on a beam kind must fail");
        assert!(err.to_string().contains("fires a beam"));
    }

    #[test]
    fn potion_heal_must_be_in_unit_interval() {
        for fraction in [0.0, 1.5] {
            let mut config = config();
            config.combat.health.player.potion_heal = fraction;
            let err = config
                .combat
                .validate(&config.actors.kinds)
                .expect_err("out-of-range potion fraction must fail");
            assert!(err.to_string().contains("potion_heal"));
        }
    }
}
