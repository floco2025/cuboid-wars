use anyhow::{Result, bail};
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::validation::{validate_non_negative_finite, validate_positive_finite};
use crate::constants::PHYSICS_EPSILON;

#[derive(Debug, Clone, Encode, Decode, Deserialize)]
pub struct CharacterGameplayConfig {
    pub collider: CharacterColliderConfig,
    pub support_probe: CharacterSupportProbeConfig,
    pub eye_height: f32,
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

    pub fn validate(&self, path: &str) -> Result<()> {
        self.collider.validate(&format!("{path}.collider"))?;
        self.support_probe.validate(&format!("{path}.support_probe"))?;
        validate_positive_finite(self.eye_height, &format!("{path}.eye_height"))
    }
}

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
pub struct CharacterColliderConfig {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    pub y_offset: f32,
    pub y_offset_anchor: CharacterColliderAnchor,
}

impl CharacterColliderConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.width, &format!("{path}.width"))?;
        validate_positive_finite(self.height, &format!("{path}.height"))?;
        validate_positive_finite(self.depth, &format!("{path}.depth"))?;
        validate_non_negative_finite(self.y_offset, &format!("{path}.y_offset"))?;
        let bottom = self.bottom_y_offset();
        if !(bottom.is_finite() && bottom >= PHYSICS_EPSILON) {
            bail!(
                "{path}.y_offset puts the collider bottom at {bottom} — must be at least {PHYSICS_EPSILON} above the entity origin so it doesn't intersect the floor (raise `y_offset`, or switch `y_offset_anchor` to `center` with a larger offset)"
            );
        }
        Ok(())
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

#[derive(Debug, Clone, Copy, Default, Encode, Decode, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CharacterColliderAnchor {
    #[default]
    Bottom,
    Center,
}

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
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
