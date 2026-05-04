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
    pub player: PlayerServerConfig,
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

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerServerConfig {
    pub projectile_damage_to_player: f32,
}

impl PlayerServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(
            self.projectile_damage_to_player,
            &format!("{path}.projectile_damage_to_player"),
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorKindServerConfig {
    // Damage this actor takes from a player projectile hit. Per-kind so a
    // tougher actor can be configured to need more shots without changing
    // the projectile.
    pub projectile_damage_from_player: f32,
    // Cooldown between an actor's death and its replacement appearing.
    // Only applies when `respawns` is true. 0.0 means immediate respawn.
    #[serde(default)]
    pub spawn_throttle_time: f32,
    // When false, the zone fills `count` actors at startup and is never
    // refilled. When true (default), deaths trigger a respawn after the
    // `spawn_throttle_time` cooldown.
    #[serde(default = "default_respawns")]
    pub respawns: bool,
    // Maximum xz-distance (meters) from the spawn zone's nearest edge before
    // the actor is considered out of bounds and walks home. Inside the zone
    // counts as 0.
    pub max_wander_distance: f32,
    pub min_direction_time: f32,
    pub max_direction_time: f32,
    pub idle_probability: f32,
    pub vision_range: f32,
    pub path_clear_lookahead_time: f32,
    pub go_to_reached_distance: f32,
    pub contact_explosion_distance: f32,
    pub explosion: ActorExplosionDamageConfig,
}

const fn default_respawns() -> bool {
    true
}

impl ActorKindServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(
            self.projectile_damage_from_player,
            &format!("{path}.projectile_damage_from_player"),
        )?;
        validate_non_negative_finite(self.spawn_throttle_time, &format!("{path}.spawn_throttle_time"))?;
        validate_positive_finite(self.max_wander_distance, &format!("{path}.max_wander_distance"))?;
        validate_positive_finite(self.min_direction_time, &format!("{path}.min_direction_time"))?;
        validate_positive_finite(self.max_direction_time, &format!("{path}.max_direction_time"))?;
        if self.min_direction_time > self.max_direction_time {
            bail!("{path}.min_direction_time must be <= {path}.max_direction_time");
        }
        validate_probability(self.idle_probability, &format!("{path}.idle_probability"))?;
        validate_positive_finite(self.vision_range, &format!("{path}.vision_range"))?;
        validate_positive_finite(
            self.path_clear_lookahead_time,
            &format!("{path}.path_clear_lookahead_time"),
        )?;
        validate_positive_finite(self.go_to_reached_distance, &format!("{path}.go_to_reached_distance"))?;
        validate_non_negative_finite(
            self.contact_explosion_distance,
            &format!("{path}.contact_explosion_distance"),
        )?;
        self.explosion.validate(&format!("{path}.explosion"))
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
