use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use bevy_ecs::prelude::Resource;
use serde::Deserialize;

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct GameplayConfig {
    pub version: u32,
    pub characters: CharactersGameplayConfig,
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
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CharacterGameplayConfig {
    pub collider: CharacterColliderConfig,
    pub support_probe: CharacterSupportProbeConfig,
    pub eye_height: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerGameplayConfig {
    #[serde(flatten)]
    pub character: CharacterGameplayConfig,
    pub walk_speed: f32,
    pub run_speed: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorGameplayConfig {
    #[serde(flatten)]
    pub character: CharacterGameplayConfig,
    pub patrol_speed: f32,
    pub chase_speed: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CharactersGameplayConfig {
    pub player: PlayerGameplayConfig,
    pub actor: ActorGameplayConfig,
}

impl CharacterGameplayConfig {
    #[must_use]
    pub const fn physics(&self) -> CharacterPhysicsConfig {
        CharacterPhysicsConfig {
            collider: self.collider,
            support_probe: self.support_probe,
        }
    }

    #[must_use]
    pub const fn eye_height(&self) -> f32 {
        self.eye_height
    }

    fn validate(&self, path: &str) -> Result<()> {
        self.collider.validate(&format!("{path}.collider"))?;
        self.support_probe.validate(&format!("{path}.support_probe"))?;
        validate_positive_finite(self.eye_height, &format!("{path}.eye_height"))
    }
}

impl PlayerGameplayConfig {
    #[must_use]
    pub const fn physics(&self) -> CharacterPhysicsConfig {
        self.character.physics()
    }

    #[must_use]
    pub const fn eye_height(&self) -> f32 {
        self.character.eye_height()
    }

    fn validate(&self, path: &str) -> Result<()> {
        self.character.validate(path)?;
        validate_positive_finite(self.walk_speed, &format!("{path}.walk_speed"))?;
        validate_positive_finite(self.run_speed, &format!("{path}.run_speed"))
    }
}

impl ActorGameplayConfig {
    #[must_use]
    pub const fn physics(&self) -> CharacterPhysicsConfig {
        self.character.physics()
    }

    #[must_use]
    pub const fn eye_height(&self) -> f32 {
        self.character.eye_height()
    }

    fn validate(&self, path: &str) -> Result<()> {
        self.character.validate(path)?;
        validate_positive_finite(self.patrol_speed, &format!("{path}.patrol_speed"))?;
        validate_positive_finite(self.chase_speed, &format!("{path}.chase_speed"))
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CharacterColliderConfig {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    pub y_offset: f32,
    #[serde(default)]
    pub y_offset_anchor: CharacterColliderAnchor,
}

impl CharacterColliderConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.width, &format!("{path}.width"))?;
        validate_positive_finite(self.height, &format!("{path}.height"))?;
        validate_positive_finite(self.depth, &format!("{path}.depth"))?;
        validate_non_negative_finite(self.y_offset, &format!("{path}.y_offset"))?;
        validate_non_negative_finite(self.bottom_y_offset(), &format!("{path}.bottom_y_offset"))
    }

    #[must_use]
    pub fn center_y_offset(self) -> f32 {
        match self.y_offset_anchor {
            CharacterColliderAnchor::Bottom => self.y_offset + self.height / 2.0,
            CharacterColliderAnchor::Center => self.y_offset,
        }
    }

    #[must_use]
    pub fn bottom_y_offset(self) -> f32 {
        match self.y_offset_anchor {
            CharacterColliderAnchor::Bottom => self.y_offset,
            CharacterColliderAnchor::Center => self.y_offset - self.height / 2.0,
        }
    }

    #[must_use]
    pub fn top_y_offset(self) -> f32 {
        self.bottom_y_offset() + self.height
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CharacterColliderAnchor {
    #[default]
    Bottom,
    Center,
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
    pub support_probe: CharacterSupportProbeConfig,
}

impl CharacterPhysicsConfig {
    #[must_use]
    pub fn collision_height(self) -> f32 {
        self.collider.height
    }

    #[must_use]
    pub fn collider_center_y(self, pos_y: f32) -> f32 {
        pos_y + self.collider.center_y_offset()
    }

    #[must_use]
    pub fn model_y_offset_from_entity_center(self, model_y_offset: f32) -> f32 {
        model_y_offset - self.collider.height / 2.0
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
