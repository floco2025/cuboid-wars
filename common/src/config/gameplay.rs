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
    pub projectiles: ProjectilesConfig,
    pub ladders: LaddersConfig,
    pub missiles: MissilesConfig,
    pub power_up_effects: PowerUpEffectsConfig,
    pub knockback: KnockbackConfig,
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
        self.projectiles.validate("projectiles")?;
        self.ladders.validate("ladders")?;
        self.missiles.validate("missiles")?;
        self.power_up_effects.validate("power_up_effects")?;
        self.knockback.validate("knockback")?;
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
    pub fn expect_actor(&self, kind: &str) -> &ActorGameplayConfig {
        self.actor(kind).expect("actor kind missing from gameplay config")
    }
}

// Projectile tuning shared verbatim by server simulation and client
// prediction — the two must integrate identical flight for the presentation
// projectiles to land where the authoritative ones do.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ProjectilesConfig {
    pub speed: f32,
    pub lifetime_secs: f32,
    // Spawn distance in front of the shooter's eye along the aim.
    pub spawn_offset: f32,
    pub radius: f32,
    // Minimum time between shots.
    pub cooldown_secs: f32,
    pub gravity: f32,
    // Air resistance coefficient (deceleration = drag * speed^2).
    pub drag_factor: f32,
    // Fraction of speed retained after a perpendicular bounce.
    pub bounce_retention: f32,
}

impl ProjectilesConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.speed, &format!("{path}.speed"))?;
        validate_positive_finite(self.lifetime_secs, &format!("{path}.lifetime_secs"))?;
        validate_positive_finite(self.spawn_offset, &format!("{path}.spawn_offset"))?;
        validate_positive_finite(self.radius, &format!("{path}.radius"))?;
        validate_non_negative_finite(self.cooldown_secs, &format!("{path}.cooldown_secs"))?;
        validate_non_negative_finite(self.gravity, &format!("{path}.gravity"))?;
        validate_non_negative_finite(self.drag_factor, &format!("{path}.drag_factor"))?;
        if !(self.bounce_retention.is_finite() && (0.0..=1.0).contains(&self.bounce_retention)) {
            bail!("{path}.bounce_retention must be within 0.0..=1.0");
        }
        Ok(())
    }
}

// Effect magnitudes of the timer power-ups. Shared: speed feeds client
// prediction, multi-shot feeds both sides' projectile spawning. (Durations
// are server-only tuning in `config/server/gameplay.json`.)
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PowerUpEffectsConfig {
    pub speed_multiplier: f32,
    pub multi_shot_count: i32,
    // Yaw between adjacent projectiles of a multi-shot arc.
    pub multi_shot_angle_degrees: f32,
}

impl PowerUpEffectsConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.speed_multiplier, &format!("{path}.speed_multiplier"))?;
        if self.multi_shot_count < 1 {
            bail!("{path}.multi_shot_count must be at least 1");
        }
        validate_positive_finite(
            self.multi_shot_angle_degrees,
            &format!("{path}.multi_shot_angle_degrees"),
        )
    }
}

// Blast knockback. Shared: the server applies the shove, the client decays
// it in prediction with the same curve.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct KnockbackConfig {
    // Horizontal shove speed at the blast center; falls off with distance
    // like damage but ignores armor (armor protects health, not momentum).
    pub blast_max_speed: f32,
    // Vertical launch speed added to `CharacterVerticalVelocity`.
    pub blast_up_speed: f32,
    // Ground-friction-style linear deceleration of the horizontal shove: a
    // hard hit that dies cleanly, no exponential crawl tail.
    pub deceleration: f32,
}

impl KnockbackConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.blast_max_speed, &format!("{path}.blast_max_speed"))?;
        validate_non_negative_finite(self.blast_up_speed, &format!("{path}.blast_up_speed"))?;
        validate_positive_finite(self.deceleration, &format!("{path}.deceleration"))
    }
}

// Ladder climb tuning. Shared: the climb is resolved inside the shared
// character step, so client prediction must integrate the same speeds the
// server does. One block for all characters — players and actors climb alike.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LaddersConfig {
    // Climb rate per unit of intent speed into (ascend) or away from
    // (descend) the ladder face — dimensionless, so walking, running, and
    // each actor kind's speed all carry into the climb rate.
    pub climb_speed_ratio: f32,
}

impl LaddersConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.climb_speed_ratio, &format!("{path}.climb_speed_ratio"))
    }
}

// Missile tuning both sides need: the client for lock detection, the HUD max,
// dry-fire prediction, and detonation VFX sizing; the server for validation
// and blast damage radius. Server-only flight tuning lives in
// `config/server/gameplay.json`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MissilesConfig {
    pub lock_max_distance: f32,
    // Aim-assist: how far the aim ray may pass from a target and still lock.
    pub lock_assist_radius: f32,
    // When true, F only fires with a validated lock; when false, an
    // unlocked shot launches an unguided missile straight along the aim
    // (like a missile whose target died).
    pub require_lock: bool,
    pub max_missiles: u32,
    pub blast_radius: f32,
}

impl MissilesConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.lock_max_distance, &format!("{path}.lock_max_distance"))?;
        validate_positive_finite(self.lock_assist_radius, &format!("{path}.lock_assist_radius"))?;
        if self.max_missiles == 0 {
            bail!("{path}.max_missiles must be at least 1");
        }
        validate_positive_finite(self.blast_radius, &format!("{path}.blast_radius"))
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
    // Initial upward velocity of a jump, m/s.
    pub jump_speed: f32,
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
        validate_positive_finite(self.jump_speed, &format!("{path}.jump_speed"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    const fn missiles_config() -> MissilesConfig {
        MissilesConfig {
            lock_max_distance: 60.0,
            lock_assist_radius: 1.2,
            require_lock: true,
            max_missiles: 3,
            blast_radius: 6.0,
        }
    }

    #[test]
    fn missiles_config_accepts_valid_values() {
        assert!(missiles_config().validate("missiles").is_ok());
    }

    #[test]
    fn missiles_config_rejects_zero_max_missiles() {
        let config = MissilesConfig {
            max_missiles: 0,
            ..missiles_config()
        };
        let err = config
            .validate("missiles")
            .expect_err("zero max_missiles passed validation");
        assert!(err.to_string().contains("max_missiles"));
    }

    #[test]
    fn missiles_config_rejects_non_positive_lock_distance() {
        let config = MissilesConfig {
            lock_max_distance: 0.0,
            ..missiles_config()
        };
        assert!(config.validate("missiles").is_err());
    }
}
