use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use bevy::prelude::Resource;
use quinn::ServerConfig;
use serde::Deserialize;

use common::config::{create_quinn_server_config, load_certs, load_private_key, resolve_actor_inheritance};

const SUPPORTED_VERSION: u32 = 1;

// ============================================================================
// Connection Configuration
// ============================================================================

pub fn configure_server() -> Result<ServerConfig> {
    let certs = load_certs()?;
    let private_key = load_private_key()?;

    let mut crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, private_key)
        .context("Failed to configure TLS")?;
    crypto.alpn_protocols = vec![b"game".to_vec()];

    create_quinn_server_config(crypto)
}

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct ServerGameplayConfig {
    pub version: u32,
    pub scoring: ScoringConfig,
    pub player: PlayerServerConfig,
    pub power_ups: PowerUpsConfig,
    pub cookies: CookiesConfig,
    pub keys: KeysConfig,
    pub actors: HashMap<String, ActorKindServerConfig>,
}

impl ServerGameplayConfig {
    pub fn load_default() -> Result<Self> {
        let config = Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/server/gameplay.json"
        )))?;
        config.validate()?;
        Ok(config)
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        let mut value: serde_json::Value =
            serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        resolve_actor_inheritance(&mut value, "actors")
            .with_context(|| format!("resolving actor inheritance in {}", path.display()))?;
        serde_json::from_value(value).with_context(|| format!("failed to deserialize {}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.version == SUPPORTED_VERSION,
            "unsupported server gameplay config version {} (expected {})",
            self.version,
            SUPPORTED_VERSION
        );
        self.player.validate("player")?;
        // No range check on scoring values — negative deltas are legal
        // (e.g., `player_death: -1` penalty), and so is zero. Just ensure
        // the section deserialized.
        let _ = &self.scoring;
        self.power_ups.validate("power_ups")?;
        self.cookies.validate("cookies")?;
        self.keys.validate("keys")?;
        if self.actors.is_empty() {
            bail!("actors must define at least one kind");
        }
        for (kind, actor) in &self.actors {
            if kind.is_empty() {
                bail!("actor kind must not be empty");
            }
            actor.validate(&format!("actors.{kind}"))?;
        }
        Ok(())
    }

    #[must_use]
    pub fn actor(&self, kind: &str) -> Option<&ActorKindServerConfig> {
        self.actors.get(kind)
    }

    #[must_use]
    pub fn validated_actor(&self, kind: &str) -> &ActorKindServerConfig {
        self.actor(kind).expect("actor kind validated at startup")
    }
}

// Global scoring deltas. Negative values are legal (e.g., a death penalty)
// and the entire block is server-only state — clients read the resulting
// `score` field via `SSnapshot` and never need the per-event point values.
#[derive(Debug, Clone, Deserialize)]
pub struct ScoringConfig {
    pub player_kill: i32,
    pub player_death: i32,
    pub cookie: i32,
    pub actor_hit: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerServerConfig {
    // Damage the player takes from a single incoming projectile.
    pub projectile_damage_taken: f32,
}

impl PlayerServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.projectile_damage_taken, &format!("{path}.projectile_damage_taken"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PowerUpsConfig {
    // Target/cap for active power-ups in the world. The spawner paces
    // spawns to maintain this many and refuses to exceed it. Capped at
    // the number of eligible floor cells so tiny test maps degrade.
    pub max_number: usize,
    // How long an uncollected power-up sits in the world before being
    // removed. Cookies and keys use `respawn_secs` instead — they're
    // hidden after collection and re-shown, not despawned.
    pub despawn_secs: f32,
    pub speed_duration_secs: f32,
    pub multi_shot_duration_secs: f32,
    pub phasing_duration_secs: f32,
    pub anti_gravity_duration_secs: f32,
}

impl PowerUpsConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.despawn_secs, &format!("{path}.despawn_secs"))?;
        validate_non_negative_finite(self.speed_duration_secs, &format!("{path}.speed_duration_secs"))?;
        validate_non_negative_finite(self.multi_shot_duration_secs, &format!("{path}.multi_shot_duration_secs"))?;
        validate_non_negative_finite(self.phasing_duration_secs, &format!("{path}.phasing_duration_secs"))?;
        validate_non_negative_finite(
            self.anti_gravity_duration_secs,
            &format!("{path}.anti_gravity_duration_secs"),
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CookiesConfig {
    pub spawning_enabled: bool,
    pub respawn_secs: f32,
}

impl CookiesConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.respawn_secs, &format!("{path}.respawn_secs"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeysConfig {
    pub respawn_secs: f32,
}

impl KeysConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.respawn_secs, &format!("{path}.respawn_secs"))
    }
}

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
    pub navigation: ActorNavigationConfig,
}

impl ActorKindServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        self.respawn.validate(&format!("{path}.respawn"))?;
        self.combat.validate(&format!("{path}.combat"))?;
        self.senses.validate(&format!("{path}.senses"))?;
        self.patrol.validate(&format!("{path}.patrol"))?;
        self.chase.validate(&format!("{path}.chase"))?;
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
}

const fn default_respawn_enabled() -> bool {
    true
}

impl ActorRespawnConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.delay_secs, &format!("{path}.delay_secs"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorCombatConfig {
    // Damage this actor takes from a single player projectile hit. Per-kind
    // so a tougher actor can need more shots without changing the projectile.
    pub projectile_damage_taken: f32,
    // Distance at which contact with a player triggers the actor's explosion.
    pub contact_explosion_distance: f32,
    pub explosion: ActorExplosionDamageConfig,
    // Bonus added to the killer's score on the lethal hit, on top of
    // `scoring.actor_hit` which fires per hit. Tougher actors should be
    // worth more — set higher for sentry, lower for mine_1.
    #[serde(default)]
    pub score_reward_on_kill: i32,
}

impl ActorCombatConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.projectile_damage_taken, &format!("{path}.projectile_damage_taken"))?;
        validate_non_negative_finite(
            self.contact_explosion_distance,
            &format!("{path}.contact_explosion_distance"),
        )?;
        self.explosion.validate(&format!("{path}.explosion"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorSensesConfig {
    pub vision_range: f32,
    // Delay after the actor reaches the last known player position before it
    // may acquire a visible player as a fresh chase target again.
    #[serde(default)]
    pub chase_reacquire_cooldown_secs: f32,
}

impl ActorSensesConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.vision_range, &format!("{path}.vision_range"))?;
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

#[derive(Debug, Clone, Deserialize)]
pub struct ActorExplosionDamageConfig {
    pub radius: f32,
    pub player_max_damage: f32,
    pub actor_max_damage: f32,
}

impl ActorExplosionDamageConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.radius, &format!("{path}.radius"))?;
        validate_non_negative_finite(self.player_max_damage, &format!("{path}.player_max_damage"))?;
        validate_non_negative_finite(self.actor_max_damage, &format!("{path}.actor_max_damage"))
    }
}

fn validate_positive_finite(value: f32, path: &str) -> Result<()> {
    if value.is_finite() && value > 0.0 {
        return Ok(());
    }
    bail!("{path} must be positive and finite, got {value}");
}

fn validate_non_negative_finite(value: f32, path: &str) -> Result<()> {
    if value.is_finite() && value >= 0.0 {
        return Ok(());
    }
    bail!("{path} must be non-negative and finite, got {value}");
}

fn validate_probability(value: f32, path: &str) -> Result<()> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        return Ok(());
    }
    bail!("{path} must be between 0 and 1, got {value}");
}
