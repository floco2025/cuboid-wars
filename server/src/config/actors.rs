use anyhow::{Result, bail};
use serde::Deserialize;

use super::validation::{validate_non_negative_finite, validate_positive_finite, validate_probability};

// Server-side per-actor-kind tuning. Fields are grouped by concern so the
// JSON reads as a self-documenting outline; cross-cutting numbers (combat,
// senses, navigation, patrol, chase, respawn) don't get jumbled into one
// flat list.
#[derive(Debug, Clone, Deserialize)]
pub struct ActorKindServerConfig {
    pub respawn: ActorRespawnConfig,
    pub combat: ActorCombatConfig,
    pub senses: ActorSensesConfig,
    pub patrol: ActorPatrolConfig,
    pub chase: ActorChaseConfig,
    // Present = laser kind: approach to standoff, fire a tracking beam, flee
    // for the cooldown. Absent = contact kind (plain chase).
    #[serde(default)]
    pub fire: Option<ActorFireConfig>,
    pub navigation: ActorNavigationConfig,
}

impl ActorKindServerConfig {
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        self.respawn.validate(&format!("{path}.respawn"))?;
        self.combat.validate(&format!("{path}.combat"))?;
        self.senses.validate(&format!("{path}.senses"))?;
        self.patrol.validate(&format!("{path}.patrol"))?;
        self.chase.validate(&format!("{path}.chase"))?;
        if let Some(fire) = &self.fire {
            fire.validate(&format!("{path}.fire"))?;
        }
        self.navigation.validate(&format!("{path}.navigation"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorRespawnConfig {
    // When false, the zone fills `count` actors at startup and is never
    // refilled. When true (default), deaths trigger a respawn after the
    // configured delay.
    #[serde(default = "default_respawn_enabled")]
    pub enabled: bool,
    // Delay between an actor's death and its replacement appearing. Only
    // applies when `enabled` is true. 0.0 means immediate respawn.
    #[serde(default)]
    pub delay_secs: f32,
    // Beam-in warning window: how long the reserved spawn is announced to
    // clients (as a ghost) before the actor materializes. Applies to initial
    // spawns too. 0.0 means actors appear instantly, without a warning.
    #[serde(default = "default_spawn_warning_secs")]
    pub warning_secs: f32,
}

const fn default_respawn_enabled() -> bool {
    true
}

const fn default_spawn_warning_secs() -> f32 {
    2.0
}

impl ActorRespawnConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.delay_secs, &format!("{path}.delay_secs"))?;
        validate_non_negative_finite(self.warning_secs, &format!("{path}.warning_secs"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorCombatConfig {
    // Fraction of incoming damage this kind blocks (0.0 = unarmored,
    // 0.9 = takes 10%). Applies to projectile hits and explosion blasts —
    // toughness lives on the victim, damage sources carry one raw number.
    pub armor: f32,
    // Distance at which contact with a player triggers the actor's explosion.
    // Absent = this kind never contact-detonates.
    #[serde(default)]
    pub contact_explosion_distance: Option<f32>,
    pub explosion: ExplosionDamageConfig,
    // Bonus added to the killer's score on the lethal hit, on top of
    // `scoring.actor_hit` which fires per hit. Tougher actors should be
    // worth more — set higher for sentry, lower for zapper.
    #[serde(default)]
    pub score_reward_on_kill: i32,
}

impl ActorCombatConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_probability(self.armor, &format!("{path}.armor"))?;
        if let Some(distance) = self.contact_explosion_distance {
            validate_non_negative_finite(distance, &format!("{path}.contact_explosion_distance"))?;
        }
        self.explosion.validate(&format!("{path}.explosion"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorSensesConfig {
    // Range gate is a box, not a sphere: a player passes when within
    // `horizontal_vision_range` on the xz plane AND `vertical_vision_range`
    // on y. LOS still has the final say.
    pub horizontal_vision_range: f32,
    pub vertical_vision_range: f32,
    // Delay after the actor reaches the last known player position before it
    // may acquire a visible player as a fresh chase target again.
    #[serde(default)]
    pub chase_reacquire_cooldown_secs: f32,
}

impl ActorSensesConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.horizontal_vision_range, &format!("{path}.horizontal_vision_range"))?;
        validate_positive_finite(self.vertical_vision_range, &format!("{path}.vertical_vision_range"))?;
        validate_non_negative_finite(
            self.chase_reacquire_cooldown_secs,
            &format!("{path}.chase_reacquire_cooldown_secs"),
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorPatrolConfig {
    // Maximum xz-distance (meters) from the spawn zone's nearest edge a
    // patrolling actor may stray before it breaks off and walks home.
    // Inside the zone counts as 0.
    pub leash: f32,
    pub min_direction_secs: f32,
    pub max_direction_secs: f32,
    pub idle_probability: f32,
}

impl ActorPatrolConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.leash, &format!("{path}.leash"))?;
        validate_positive_finite(self.min_direction_secs, &format!("{path}.min_direction_secs"))?;
        validate_positive_finite(self.max_direction_secs, &format!("{path}.max_direction_secs"))?;
        if self.min_direction_secs > self.max_direction_secs {
            bail!("{path}.min_direction_secs must be <= {path}.max_direction_secs");
        }
        validate_probability(self.idle_probability, &format!("{path}.idle_probability"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorChaseConfig {
    // Maximum xz-distance (meters) from the spawn zone's nearest edge a
    // chasing actor may stray before it breaks off the chase (triggering
    // `senses.chase_reacquire_cooldown_secs`) and walks home. Typically larger
    // than `patrol.leash` so a predator can pursue a fleeing player past
    // its normal roam.
    pub leash: f32,
}

impl ActorChaseConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.leash, &format!("{path}.leash"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorFireConfig {
    // Approach stops (holds position) once within this xz-distance of the
    // target; firing doesn't require it — only `range`.
    pub standoff_distance: f32,
    // Maximum xz-distance at which the beam can start and keep damaging.
    pub range: f32,
    // Length of one beam burst.
    pub duration_secs: f32,
    // Post-burst lockout; also how long the actor flees to hide.
    pub cooldown_secs: f32,
    // Raw beam damage per second of contact; the victim's armor applies.
    pub damage_per_second: f32,
}

impl ActorFireConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.standoff_distance, &format!("{path}.standoff_distance"))?;
        validate_positive_finite(self.range, &format!("{path}.range"))?;
        if self.standoff_distance > self.range {
            bail!("{path}.standoff_distance must be <= {path}.range");
        }
        validate_positive_finite(self.duration_secs, &format!("{path}.duration_secs"))?;
        validate_non_negative_finite(self.cooldown_secs, &format!("{path}.cooldown_secs"))?;
        validate_positive_finite(self.damage_per_second, &format!("{path}.damage_per_second"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorNavigationConfig {
    pub path_clear_lookahead_secs: f32,
    pub go_to_reached_distance: f32,
}

impl ActorNavigationConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(
            self.path_clear_lookahead_secs,
            &format!("{path}.path_clear_lookahead_secs"),
        )?;
        validate_positive_finite(self.go_to_reached_distance, &format!("{path}.go_to_reached_distance"))
    }
}

// One blast, one number: everything caught in the radius uses the shared
// full-strength core and quadratic falloff to zero at the rim.
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

    fn ok_fire() -> ActorFireConfig {
        ActorFireConfig {
            standoff_distance: 6.0,
            range: 15.0,
            duration_secs: 1.0,
            cooldown_secs: 5.0,
            damage_per_second: 75.0,
        }
    }

    #[test]
    fn default_config_loads_zapper_fire_group() {
        let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let zapper = config.expect_actor("zapper");
        let fire = zapper.fire.as_ref().expect("zapper fire group missing from config");
        assert_eq!(fire.duration_secs, 2.0);
        assert_eq!(fire.cooldown_secs, 5.0);
        assert_eq!(zapper.combat.contact_explosion_distance, None);

        // The contact kinds must not have inherited the laser behavior.
        let mine = config.expect_actor("mine");
        assert!(mine.fire.is_none());
        assert_eq!(mine.combat.contact_explosion_distance, Some(0.4));
        assert!(config.expect_actor("sentry").fire.is_none());
    }

    #[test]
    fn fire_config_rejects_standoff_beyond_range() {
        let fire = ActorFireConfig {
            standoff_distance: 20.0,
            ..ok_fire()
        };
        let err = fire
            .validate("actors.test.fire")
            .expect_err("standoff > range must fail");
        assert!(err.to_string().contains("standoff_distance"));
    }

    #[test]
    fn fire_config_rejects_non_positive_duration() {
        let fire = ActorFireConfig {
            duration_secs: 0.0,
            ..ok_fire()
        };
        fire.validate("actors.test.fire").expect_err("zero duration must fail");
    }
}
