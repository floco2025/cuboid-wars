use anyhow::{Result, bail};
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::validation::{validate_non_negative_finite, validate_positive_finite};

#[derive(Debug, Clone, Encode, Decode, Deserialize)]
pub struct CharacterGameplayConfig {
    pub movement_collider: MovementColliderConfig,
    pub hitbox: HitboxConfig,
    pub eye_height: f32,
}

impl CharacterGameplayConfig {
    #[must_use]
    pub const fn physics(&self) -> CharacterPhysicsConfig {
        CharacterPhysicsConfig {
            movement_collider: self.movement_collider,
            hitbox: self.hitbox,
        }
    }

    #[must_use]
    pub const fn eye_height(&self) -> f32 {
        self.eye_height
    }

    pub fn validate(&self, path: &str) -> Result<()> {
        self.movement_collider.validate(&format!("{path}.movement_collider"))?;
        self.hitbox.validate(&format!("{path}.hitbox"))?;
        validate_positive_finite(self.eye_height, &format!("{path}.eye_height"))
    }
}

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementColliderConfig {
    pub diameter: f32,
    pub height: f32,
}

impl MovementColliderConfig {
    pub fn validate(self, path: &str) -> Result<()> {
        validate_positive_finite(self.diameter, &format!("{path}.diameter"))?;
        validate_positive_finite(self.height, &format!("{path}.height"))?;
        if self.height < self.diameter {
            bail!("{path}.height must be at least diameter");
        }
        Ok(())
    }

    #[must_use]
    pub const fn radius(self) -> f32 {
        self.diameter / 2.0
    }

    #[must_use]
    pub fn segment_half_height(self) -> f32 {
        (self.height - self.diameter) / 2.0
    }
}

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitboxConfig {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    pub bottom_offset: f32,
}

impl HitboxConfig {
    fn validate(self, path: &str) -> Result<()> {
        validate_positive_finite(self.width, &format!("{path}.width"))?;
        validate_positive_finite(self.height, &format!("{path}.height"))?;
        validate_positive_finite(self.depth, &format!("{path}.depth"))?;
        validate_non_negative_finite(self.bottom_offset, &format!("{path}.bottom_offset"))
    }

    #[must_use]
    pub fn center_y_offset(self) -> f32 {
        self.bottom_offset + self.height / 2.0
    }

    #[must_use]
    pub fn top_y_offset(self) -> f32 {
        self.bottom_offset + self.height
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CharacterPhysicsConfig {
    pub movement_collider: MovementColliderConfig,
    pub hitbox: HitboxConfig,
}

impl CharacterPhysicsConfig {
    #[must_use]
    pub fn hitbox_center_y(self, pos_y: f32) -> f32 {
        pos_y + self.hitbox.center_y_offset()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capsule_dimensions_reject_invalid_values_and_allow_a_sphere() {
        for (diameter, height) in [
            (0.0, 1.0),
            (-0.1, 1.0),
            (f32::NAN, 1.0),
            (1.0, f32::INFINITY),
            (1.0, 0.9),
        ] {
            assert!(
                MovementColliderConfig { diameter, height }
                    .validate("player.movement_collider")
                    .is_err()
            );
        }
        assert!(
            MovementColliderConfig {
                diameter: 1.0,
                height: 1.0
            }
            .validate("player.movement_collider")
            .is_ok()
        );
    }
}
