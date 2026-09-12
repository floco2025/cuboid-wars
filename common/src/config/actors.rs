use anyhow::{Result, bail};
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::{CharacterGameplayConfig, CharacterPhysicsConfig, validation::validate_positive_finite};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorLocomotion {
    Ground,
    Flying,
}

#[derive(Debug, Clone, Encode, Decode, Deserialize)]
pub struct ActorGameplayConfig {
    #[serde(flatten)]
    pub character: CharacterGameplayConfig,
    pub can_use_ladders: bool,
    pub immovable: bool,
    pub locomotion: ActorLocomotion,
    // A gun above a pedestal fires from its head rather than its collision center.
    #[serde(default)]
    pub beam_origin_height: Option<f32>,
}

impl ActorGameplayConfig {
    pub const fn flies(&self) -> bool {
        matches!(self.locomotion, ActorLocomotion::Flying)
    }

    pub const fn physics(&self) -> CharacterPhysicsConfig {
        self.character.physics()
    }

    pub const fn eye_height(&self) -> f32 {
        self.character.eye_height()
    }

    pub fn beam_origin_y_offset(&self) -> f32 {
        self.beam_origin_height
            .unwrap_or_else(|| self.physics().hitbox.center_y_offset())
    }

    pub fn validate(&self, path: &str) -> Result<()> {
        self.character.validate(path)?;
        if self.flies() && (self.can_use_ladders || self.immovable) {
            bail!("{path}: flying actors cannot use ladders or be immovable");
        }
        if let Some(height) = self.beam_origin_height {
            validate_positive_finite(height, &format!("{path}.beam_origin_height"))?;
        }
        if self.immovable && self.can_use_ladders {
            bail!("{path}.can_use_ladders must be false for immovable actors");
        }
        Ok(())
    }
}
