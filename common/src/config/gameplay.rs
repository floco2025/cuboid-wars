use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use bevy_ecs::prelude::Resource;
use serde::Deserialize;

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct GameplayConfig {
    pub version: u32,
    pub characters: CharacterGameplayConfig,
}

impl GameplayConfig {
    pub fn load_default() -> Result<Self> {
        let config = Self::load_from_path(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/gameplay.json")))?;
        config.validate()?;
        Ok(config)
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("unsupported gameplay config version {}", self.version);
        }
        self.characters.player.validate("characters.player")?;
        self.characters.actor.validate("characters.actor")?;
        validate_positive_finite(
            self.characters.player.eye_height_ratio,
            "characters.player.eye_height_ratio",
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CharacterGameplayConfig {
    pub player: PlayerGameplayConfig,
    pub actor: ActorGameplayConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerGameplayConfig {
    pub collider: CharacterColliderConfig,
    pub low_obstacle_clearance: f32,
    pub support_probe: CharacterSupportProbeConfig,
    pub eye_height_ratio: f32,
    pub speed: f32,
}

impl PlayerGameplayConfig {
    #[must_use]
    pub const fn physics(&self) -> CharacterPhysicsConfig {
        CharacterPhysicsConfig {
            collider: self.collider,
            low_obstacle_clearance: self.low_obstacle_clearance,
            support_probe: self.support_probe,
        }
    }

    #[must_use]
    pub fn eye_height(&self) -> f32 {
        self.collider.height * self.eye_height_ratio
    }

    fn validate(&self, path: &str) -> Result<()> {
        self.collider.validate(&format!("{path}.collider"))?;
        self.support_probe.validate(&format!("{path}.support_probe"))?;
        validate_non_negative_finite(self.low_obstacle_clearance, &format!("{path}.low_obstacle_clearance"))?;
        validate_positive_finite(self.speed, &format!("{path}.speed"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorGameplayConfig {
    pub collider: CharacterColliderConfig,
    pub low_obstacle_clearance: f32,
    pub support_probe: CharacterSupportProbeConfig,
    pub speed: f32,
}

impl ActorGameplayConfig {
    #[must_use]
    pub const fn physics(&self) -> CharacterPhysicsConfig {
        CharacterPhysicsConfig {
            collider: self.collider,
            low_obstacle_clearance: self.low_obstacle_clearance,
            support_probe: self.support_probe,
        }
    }

    fn validate(&self, path: &str) -> Result<()> {
        self.collider.validate(&format!("{path}.collider"))?;
        self.support_probe.validate(&format!("{path}.support_probe"))?;
        validate_non_negative_finite(self.low_obstacle_clearance, &format!("{path}.low_obstacle_clearance"))?;
        validate_positive_finite(self.speed, &format!("{path}.speed"))
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CharacterColliderConfig {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
}

impl CharacterColliderConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.width, &format!("{path}.width"))?;
        validate_positive_finite(self.height, &format!("{path}.height"))?;
        validate_positive_finite(self.depth, &format!("{path}.depth"))
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CharacterSupportProbeConfig {
    pub width: f32,
    pub depth: f32,
}

impl CharacterSupportProbeConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.width, &format!("{path}.width"))?;
        validate_positive_finite(self.depth, &format!("{path}.depth"))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CharacterPhysicsConfig {
    pub collider: CharacterColliderConfig,
    pub low_obstacle_clearance: f32,
    pub support_probe: CharacterSupportProbeConfig,
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
