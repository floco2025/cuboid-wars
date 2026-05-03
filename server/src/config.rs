use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use bevy::prelude::Resource;
use quinn::ServerConfig;
use serde::Deserialize;

use common::config::{create_quinn_server_config, load_certs, load_private_key};

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
    pub actors: ActorBehaviorConfig,
    pub damage: DamageConfig,
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
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.version == SUPPORTED_VERSION,
            "unsupported server gameplay config version {} (expected {})",
            self.version,
            SUPPORTED_VERSION
        );
        self.actors.validate("actors")?;
        self.damage.validate("damage")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorBehaviorConfig {
    pub initial_count: u32,
    pub min_direction_time: f32,
    pub max_direction_time: f32,
    pub idle_chance: f32,
    pub vision_range: f32,
    pub direct_path_probe_time: f32,
    pub go_to_reached_distance: f32,
}

impl ActorBehaviorConfig {
    fn validate(&self, path: &str) -> Result<()> {
        if self.initial_count == 0 {
            bail!("{path}.initial_count must be greater than zero");
        }
        validate_positive_finite(self.min_direction_time, &format!("{path}.min_direction_time"))?;
        validate_positive_finite(self.max_direction_time, &format!("{path}.max_direction_time"))?;
        if self.min_direction_time > self.max_direction_time {
            bail!("{path}.min_direction_time must be <= {path}.max_direction_time");
        }
        validate_probability(self.idle_chance, &format!("{path}.idle_chance"))?;
        validate_positive_finite(self.vision_range, &format!("{path}.vision_range"))?;
        validate_positive_finite(self.direct_path_probe_time, &format!("{path}.direct_path_probe_time"))?;
        validate_positive_finite(self.go_to_reached_distance, &format!("{path}.go_to_reached_distance"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DamageConfig {
    pub player_projectile_to_player: f32,
    pub player_projectile_to_actor: f32,
    pub actor_contact_to_player_per_second: f32,
}

impl DamageConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(
            self.player_projectile_to_player,
            &format!("{path}.player_projectile_to_player"),
        )?;
        validate_non_negative_finite(
            self.player_projectile_to_actor,
            &format!("{path}.player_projectile_to_actor"),
        )?;
        validate_non_negative_finite(
            self.actor_contact_to_player_per_second,
            &format!("{path}.actor_contact_to_player_per_second"),
        )
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
