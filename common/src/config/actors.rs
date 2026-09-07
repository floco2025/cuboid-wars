use anyhow::Result;
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::{CharacterGameplayConfig, CharacterPhysicsConfig};

#[derive(Debug, Clone, Encode, Decode, Deserialize)]
pub struct ActorGameplayConfig {
    #[serde(flatten)]
    pub character: CharacterGameplayConfig,
    pub can_use_ladders: bool,
}

impl ActorGameplayConfig {
    pub const fn physics(&self) -> CharacterPhysicsConfig {
        self.character.physics()
    }

    pub const fn eye_height(&self) -> f32 {
        self.character.eye_height()
    }

    pub fn validate(&self, path: &str) -> Result<()> {
        self.character.validate(path)
    }
}
