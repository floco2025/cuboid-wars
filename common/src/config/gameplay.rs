use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use bevy_ecs::prelude::Resource;
use serde::Deserialize;

use super::inheritance::resolve_actor_inheritance;
use crate::constants::PHYSICS_EPSILON;

const SUPPORTED_VERSION: u32 = 1;

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct GameplayConfig {
    pub version: u32,
    pub player: PlayerGameplayConfig,
    pub actors: HashMap<String, ActorGameplayConfig>,
    // Ordered list of barrier / key kind ids. Order is the stable
    // `BarrierKindId` index used on the wire. Visuals (colors) live in
    // `config/client/assets.json` so the server stays presentation-free.
    #[serde(default)]
    pub barrier_kinds: Vec<String>,
}

impl GameplayConfig {
    pub fn load_default() -> Result<Self> {
        let config = Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/common/gameplay.json"
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
            "unsupported gameplay config version {} (expected {})",
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
    pub fn actor(&self, kind: &str) -> Option<&ActorGameplayConfig> {
        self.actors.get(kind)
    }

    #[must_use]
    pub fn validated_actor(&self, kind: &str) -> &ActorGameplayConfig {
        self.actor(kind).expect("actor kind missing from gameplay config")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CharacterGameplayConfig {
    pub collider: CharacterColliderConfig,
    pub support_probe: CharacterSupportProbeConfig,
    pub eye_height: f32,
    pub health: CharacterHealthConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerGameplayConfig {
    #[serde(flatten)]
    pub character: CharacterGameplayConfig,
    pub walk_speed: f32,
    pub run_speed: f32,
    // How long a player stays "dead" (entity despawned, red overlay on the
    // local client) before being respawned at a fresh spawn-zone cell.
    pub respawn_delay_secs: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorGameplayConfig {
    #[serde(flatten)]
    pub character: CharacterGameplayConfig,
    pub patrol_speed: f32,
    pub chase_speed: f32,
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
        validate_positive_finite(self.eye_height, &format!("{path}.eye_height"))?;
        self.health.validate(&format!("{path}.health"))
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

    #[must_use]
    pub const fn health(&self) -> CharacterHealthConfig {
        self.character.health
    }

    fn validate(&self, path: &str) -> Result<()> {
        self.character.validate(path)?;
        validate_positive_finite(self.walk_speed, &format!("{path}.walk_speed"))?;
        validate_positive_finite(self.run_speed, &format!("{path}.run_speed"))?;
        validate_positive_finite(self.respawn_delay_secs, &format!("{path}.respawn_delay_secs"))
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

    #[must_use]
    pub const fn health(&self) -> CharacterHealthConfig {
        self.character.health
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
        // The collider's bottom must sit strictly above the entity origin
        // (the floor at the character's feet). A bottom of zero leaves the
        // bottom face coincident with the floor, which Rapier's ground-hit
        // probe + autostep treat as a wall contact — the character cannot
        // move at all. Resolve `y_offset` (the JSON field the author edits)
        // upward in the error message.
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

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CharacterHealthConfig {
    pub max: f32,
    pub regeneration_per_second: f32,
}

impl CharacterHealthConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.max, &format!("{path}.max"))?;
        validate_non_negative_finite(self.regeneration_per_second, &format!("{path}.regeneration_per_second"))
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
