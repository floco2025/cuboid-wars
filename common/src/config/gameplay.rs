use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail};
use bevy_ecs::prelude::Resource;
use serde::Deserialize;

use crate::constants::PHYSICS_EPSILON;

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct GameplayConfig {
    pub player: PlayerGameplayConfig,
    pub movement: MovementConfig,
    pub projectiles: ProjectilesConfig,
    pub missiles: MissilesConfig,
    pub portals: PortalsConfig,
    pub actors: HashMap<String, CharacterGameplayConfig>,
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
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        self.player.validate("player")?;
        self.projectiles.validate("projectiles")?;
        self.missiles.validate("missiles")?;
        self.portals.validate("portals")?;
        if self.actors.is_empty() {
            bail!("actors must define at least one kind");
        }
        for (kind, actor) in &self.actors {
            if kind.is_empty() {
                bail!("actor kind must not be empty");
            }
            actor.validate(&format!("actors.{kind}"))?;
        }
        self.movement.validate(&self.actors)
    }

    #[must_use]
    pub fn actor(&self, kind: &str) -> Option<&CharacterGameplayConfig> {
        self.actors.get(kind)
    }

    #[must_use]
    pub fn expect_actor(&self, kind: &str) -> &CharacterGameplayConfig {
        self.actor(kind).expect("actor kind missing from gameplay config")
    }
}

// Every speed in the game, in m/s, on one screen: who outruns whom.
#[derive(Debug, Clone, Deserialize)]
pub struct MovementConfig {
    pub player: PlayerMovementConfig,
    pub actors: HashMap<String, ActorMovementConfig>,
    pub missile_speed: f32,
    pub projectile_speed: f32,
    // Climb rate per unit of intent speed into (ascend) or away from
    // (descend) the ladder face — dimensionless, so walking, running, and
    // each actor kind's speed all carry into the climb rate.
    pub ladder_climb_ratio: f32,
    pub knockback: KnockbackConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PlayerMovementConfig {
    pub walk_speed: f32,
    pub run_speed: f32,
    // Walk/run multiplier while the speed power-up is active.
    pub speed_power_up: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ActorMovementConfig {
    pub roam_speed: f32,
    pub active_speed: f32,
}

impl MovementConfig {
    #[must_use]
    pub fn expect_actor(&self, kind: &str) -> &ActorMovementConfig {
        self.actors.get(kind).expect("actor kind missing from movement.actors")
    }

    fn validate(&self, actors: &HashMap<String, CharacterGameplayConfig>) -> Result<()> {
        validate_positive_finite(self.player.walk_speed, "movement.player.walk_speed")?;
        validate_positive_finite(self.player.run_speed, "movement.player.run_speed")?;
        validate_positive_finite(self.player.speed_power_up, "movement.player.speed_power_up")?;
        validate_covers_actor_kinds(self.actors.keys(), actors, "movement.actors")?;
        for (kind, actor) in &self.actors {
            validate_positive_finite(actor.roam_speed, &format!("movement.actors.{kind}.roam_speed"))?;
            validate_positive_finite(actor.active_speed, &format!("movement.actors.{kind}.active_speed"))?;
        }
        validate_positive_finite(self.missile_speed, "movement.missile_speed")?;
        validate_positive_finite(self.projectile_speed, "movement.projectile_speed")?;
        validate_positive_finite(self.ladder_climb_ratio, "movement.ladder_climb_ratio")?;
        self.knockback.validate("movement.knockback")
    }
}

// Projectile tuning shared verbatim by server simulation and client
// prediction — the two must integrate identical flight for the presentation
// projectiles to land where the authoritative ones do.
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectilesConfig {
    pub lifetime_secs: f32,
    // Spawn distance in front of the shooter's eye along the aim.
    pub spawn_offset: f32,
    pub radius: f32,
    // Minimum time between shots.
    pub cooldown_secs: f32,
    // Multiplier on the map's base gravity: 0 = no drop, 1 = falls like a
    // character. Not touched by the low-gravity power-up.
    pub gravity_scale: f32,
    // Air resistance coefficient (deceleration = drag * speed^2).
    pub drag_factor: f32,
    // Fraction of speed retained after a perpendicular bounce.
    pub bounce_retention: f32,
    pub multi_shot: MultiShotConfig,
}

impl ProjectilesConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.lifetime_secs, &format!("{path}.lifetime_secs"))?;
        validate_positive_finite(self.spawn_offset, &format!("{path}.spawn_offset"))?;
        validate_positive_finite(self.radius, &format!("{path}.radius"))?;
        validate_non_negative_finite(self.cooldown_secs, &format!("{path}.cooldown_secs"))?;
        validate_non_negative_finite(self.gravity_scale, &format!("{path}.gravity_scale"))?;
        validate_non_negative_finite(self.drag_factor, &format!("{path}.drag_factor"))?;
        if !(self.bounce_retention.is_finite() && (0.0..=1.0).contains(&self.bounce_retention)) {
            bail!("{path}.bounce_retention must be within 0.0..=1.0");
        }
        Ok(())
    }
}

const MULTI_SHOT_MAX_SHOTS: usize = 9;

// Multi-shot patterns parsed once at load. `allowed_patterns` is the ordered
// subset cycled in-game; other entries stay available for later tuning. Each
// pattern is a stencil of rows top-down with columns
// `spread_degrees × column_scale` apart in yaw and rows
// `spread_degrees × row_scale` apart in pitch — so one number sizes every
// pattern, and the scales only reshape the grid. `x` fires, `.` does not,
// and the one required `o` fires from the exact aim. A pattern name ending
// in `_<n>` must fire exactly n shots.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "MultiShotSource")]
pub struct MultiShotConfig {
    allowed_patterns: Vec<String>,
    patterns: HashMap<String, MultiShotPatternConfig>,
}

#[derive(Debug, Clone)]
pub struct MultiShotPatternConfig {
    shots: Vec<(f32, f32)>,
}

#[derive(Deserialize)]
struct MultiShotSource {
    spread_degrees: f32,
    allowed_patterns: Vec<String>,
    patterns: HashMap<String, MultiShotPattern>,
}

const fn full_scale() -> f32 {
    1.0
}

#[derive(Clone, Deserialize)]
struct MultiShotPattern {
    #[serde(default = "full_scale")]
    column_scale: f32,
    #[serde(default = "full_scale")]
    row_scale: f32,
    stencil: Vec<String>,
}

impl TryFrom<MultiShotSource> for MultiShotConfig {
    type Error = anyhow::Error;

    fn try_from(source: MultiShotSource) -> Result<Self> {
        validate_positive_finite(source.spread_degrees, "multi_shot.spread_degrees")?;
        if source.allowed_patterns.is_empty() {
            bail!("multi_shot.allowed_patterns must contain at least one pattern");
        }
        let mut seen = HashSet::new();
        for name in &source.allowed_patterns {
            if !seen.insert(name) {
                bail!("multi_shot.allowed_patterns contains duplicate pattern {name:?}");
            }
            if !source.patterns.contains_key(name) {
                bail!("multi_shot.allowed_patterns contains unknown pattern {name:?}");
            }
        }

        let mut patterns = HashMap::new();
        for (name, pattern) in &source.patterns {
            let path = format!("multi_shot.patterns.{name}");
            validate_positive_finite(pattern.column_scale, &format!("{path}.column_scale"))?;
            validate_positive_finite(pattern.row_scale, &format!("{path}.row_scale"))?;
            let config = MultiShotPatternConfig::from_stencil(
                &path,
                source.spread_degrees * pattern.column_scale,
                source.spread_degrees * pattern.row_scale,
                &pattern.stencil,
            )?;
            if let Some((_, digits)) = name.rsplit_once('_')
                && let Ok(count) = digits.parse::<usize>()
                && count != config.shots().len()
            {
                bail!(
                    "{path} fires {} shots, not the {count} its name claims",
                    config.shots().len()
                );
            }
            patterns.insert(name.clone(), config);
        }
        Ok(Self {
            allowed_patterns: source.allowed_patterns,
            patterns,
        })
    }
}

impl MultiShotConfig {
    #[must_use]
    pub fn allowed_patterns(&self) -> &[String] {
        &self.allowed_patterns
    }

    #[must_use]
    pub fn pattern(&self, name: &str) -> Option<&MultiShotPatternConfig> {
        self.patterns
            .get(name)
            .filter(|_| self.allowed_patterns.iter().any(|allowed| allowed == name))
    }

    // The `index`-th allowed pattern, in cycle order.
    #[must_use]
    pub fn allowed_pattern(&self, index: usize) -> Option<(&str, &MultiShotPatternConfig)> {
        let name = self.allowed_patterns.get(index)?;
        let pattern = self
            .patterns
            .get(name)
            .expect("allowed multi-shot pattern missing after config validation");
        Some((name, pattern))
    }

    #[must_use]
    pub fn first_allowed_pattern(&self) -> (&str, &MultiShotPatternConfig) {
        self.allowed_pattern(0)
            .expect("allowed multi-shot patterns missing after config validation")
    }

    #[cfg(test)]
    pub(crate) fn from_stencil(path: &str, column_degrees: f32, row_degrees: f32, stencil: &[String]) -> Result<Self> {
        let pattern = MultiShotPatternConfig::from_stencil(path, column_degrees, row_degrees, stencil)?;
        Ok(Self {
            allowed_patterns: vec!["test".to_owned()],
            patterns: HashMap::from([("test".to_owned(), pattern)]),
        })
    }

    #[cfg(test)]
    fn shots(&self) -> &[(f32, f32)] {
        self.first_allowed_pattern().1.shots()
    }
}

impl MultiShotPatternConfig {
    // Positive yaw turns left (`direction_from_yaw_pitch`), so columns
    // further right go negative.
    fn from_stencil(path: &str, column_degrees: f32, row_degrees: f32, stencil: &[String]) -> Result<Self> {
        validate_positive_finite(column_degrees, &format!("{path}.column_degrees"))?;
        validate_positive_finite(row_degrees, &format!("{path}.row_degrees"))?;
        let Some(width) = stencil.first().map(|row| row.chars().count()) else {
            bail!("{path}.stencil must have at least one row");
        };
        if width == 0 || stencil.iter().any(|row| row.chars().count() != width) {
            bail!("{path}.stencil rows must all have the same non-zero width");
        }

        let mut cells = Vec::new();
        let mut aim = None;
        for (row, line) in stencil.iter().enumerate() {
            for (col, cell) in line.chars().enumerate() {
                let (fires, anchors) = match cell {
                    'x' => (true, false),
                    'o' => (true, true),
                    '.' => (false, false),
                    other => bail!("{path}.stencil may only contain 'x', 'o' and '.', found {other:?}"),
                };
                if anchors && aim.replace((col as f32, row as f32)).is_some() {
                    bail!("{path}.stencil may mark the aim only once");
                }
                if fires {
                    cells.push((col as f32, row as f32));
                }
            }
        }
        let Some((aim_col, aim_row)) = aim else {
            bail!("{path}.stencil must contain exactly one 'o' center shot");
        };

        let column_step = column_degrees.to_radians();
        let row_step = row_degrees.to_radians();
        let shots: Vec<(f32, f32)> = cells
            .into_iter()
            .map(|(col, row)| (-(col - aim_col) * column_step, (aim_row - row) * row_step))
            .collect();
        if shots.len() > MULTI_SHOT_MAX_SHOTS {
            bail!(
                "{path}.stencil has {} shots; max is {MULTI_SHOT_MAX_SHOTS}",
                shots.len()
            );
        }
        Ok(Self { shots })
    }

    // (yaw, pitch) offsets in radians, one per shot.
    #[must_use]
    pub fn shots(&self) -> &[(f32, f32)] {
        &self.shots
    }
}

// Blast knockback. Shared: the server applies the shove, the client decays
// it in prediction with the same curve.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct KnockbackConfig {
    // Horizontal shove speed at the blast center; falls off with distance
    // like damage.
    pub max_speed: f32,
    // Vertical launch speed added to `CharacterVerticalVelocity`.
    pub up_speed: f32,
    // Ground-friction-style linear deceleration of the horizontal shove: a
    // hard hit that dies cleanly, no exponential crawl tail.
    pub deceleration: f32,
}

impl KnockbackConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.max_speed, &format!("{path}.max_speed"))?;
        validate_non_negative_finite(self.up_speed, &format!("{path}.up_speed"))?;
        validate_positive_finite(self.deceleration, &format!("{path}.deceleration"))
    }
}

// Missile tuning both sides need: the client for lock detection, the HUD max,
// and dry-fire prediction; the server for validation. Flight and blast
// tuning live in `config/server/gameplay.json`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MissilesConfig {
    pub lock_range: f32,
    // Aim-assist: how far the aim ray may pass from a target and still lock.
    pub lock_assist_radius: f32,
    // When true, F only fires with a validated lock; when false, an
    // unlocked shot launches an unguided missile straight along the aim
    // (like a missile whose target died).
    pub require_lock: bool,
    pub max_missiles: u32,
}

impl MissilesConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.lock_range, &format!("{path}.lock_range"))?;
        validate_positive_finite(self.lock_assist_radius, &format!("{path}.lock_assist_radius"))?;
        if self.max_missiles == 0 {
            bail!("{path}.max_missiles must be at least 1");
        }
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
    // Take-off speed, m/s. Jump height is v² / 2g for the map's gravity, so
    // a lower-gravity map gives higher jumps.
    pub jump_speed: f32,
    // How long a player stays "dead" (entity despawned, red overlay on the
    // local client) before being respawned at a fresh spawn-zone cell.
    pub respawn_secs: f32,
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
        validate_positive_finite(self.jump_speed, &format!("{path}.jump_speed"))?;
        validate_positive_finite(self.respawn_secs, &format!("{path}.respawn_secs"))
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

// A per-actor-kind map must name every configured kind (a missing entry
// silently defaulting is the footgun) and nothing else (a typo).
fn validate_covers_actor_kinds<'a>(
    keys: impl Iterator<Item = &'a String>,
    actors: &HashMap<String, CharacterGameplayConfig>,
    path: &str,
) -> Result<()> {
    let keys: HashSet<&String> = keys.collect();
    for kind in actors.keys() {
        if !keys.contains(kind) {
            bail!("{path} is missing actor kind {kind:?}");
        }
    }
    for kind in keys {
        if !actors.contains_key(kind) {
            bail!("{path} contains unknown actor kind {kind:?}");
        }
    }
    Ok(())
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

// Portal-gun tuning both sides need: the client predicts placement with the
// same numbers the server validates with, so fire vs dry-fire feedback is
// instant. Aperture geometry and traversal thresholds are fixed shared
// constants.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PortalsConfig {
    // Max placement distance of the portal shot's surface ray, m.
    pub range: f32,
}

impl PortalsConfig {
    fn validate(&self, path: &str) -> Result<()> {
        if !(self.range.is_finite() && self.range > 0.0) {
            bail!("{path}.range must be positive, got {}", self.range);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_rejects_missing_actor_kind() {
        let mut config = GameplayConfig::load_default().expect("default gameplay config should load");
        config.movement.actors.remove("mine");
        let err = config
            .movement
            .validate(&config.actors)
            .expect_err("missing kind must fail");
        assert!(err.to_string().contains("movement.actors"));
        assert!(err.to_string().contains("mine"));
    }

    #[test]
    fn movement_rejects_unknown_actor_kind() {
        let mut config = GameplayConfig::load_default().expect("default gameplay config should load");
        let zapper = *config.movement.expect_actor("zapper");
        config.movement.actors.insert("banana".to_owned(), zapper);
        let err = config
            .movement
            .validate(&config.actors)
            .expect_err("unknown kind must fail");
        assert!(err.to_string().contains("movement.actors"));
        assert!(err.to_string().contains("banana"));
    }

    const fn missiles_config() -> MissilesConfig {
        MissilesConfig {
            lock_range: 60.0,
            lock_assist_radius: 1.2,
            require_lock: true,
            max_missiles: 3,
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
            lock_range: 0.0,
            ..missiles_config()
        };
        assert!(config.validate("missiles").is_err());
    }

    fn multi_shot(stencil: &[&str]) -> Result<MultiShotConfig> {
        let rows: Vec<String> = stencil.iter().map(|row| (*row).to_owned()).collect();
        MultiShotConfig::from_stencil("multi_shot", 2.0, 3.0, &rows)
    }

    #[test]
    fn multi_shot_selects_and_scales_a_named_pattern() {
        let patterns = HashMap::from([(
            "line".to_owned(),
            MultiShotPattern {
                column_scale: 1.5,
                row_scale: 1.0,
                stencil: vec!["xo".to_owned()],
            },
        )]);
        let selected = MultiShotConfig::try_from(MultiShotSource {
            spread_degrees: 2.0,
            allowed_patterns: vec!["line".to_owned()],
            patterns: patterns.clone(),
        })
        .expect("named pattern rejected");
        // One column step left of the aim, at spread × scale = 3° per column.
        let (yaw, pitch) = selected.shots()[0];
        assert!((yaw - 3.0_f32.to_radians()).abs() < 1e-6 && pitch == 0.0);

        let missing = MultiShotConfig::try_from(MultiShotSource {
            spread_degrees: 2.0,
            allowed_patterns: vec!["ring".to_owned()],
            patterns,
        });
        assert!(missing.expect_err("unknown name accepted").to_string().contains("ring"));
    }

    #[test]
    fn multi_shot_allowed_patterns_are_ordered_and_may_leave_dormant_patterns() {
        let pattern = || MultiShotPattern {
            column_scale: 1.0,
            row_scale: 1.0,
            stencil: vec!["xo".to_owned()],
        };
        let patterns = HashMap::from([
            ("dormant_2".to_owned(), pattern()),
            ("second_2".to_owned(), pattern()),
            ("first_2".to_owned(), pattern()),
        ]);
        let config = MultiShotConfig::try_from(MultiShotSource {
            spread_degrees: 2.0,
            allowed_patterns: vec!["first_2".to_owned(), "second_2".to_owned()],
            patterns: patterns.clone(),
        })
        .expect("ordered allowed patterns rejected");
        assert_eq!(config.allowed_patterns(), ["first_2", "second_2"]);
        assert!(config.pattern("dormant_2").is_none());

        let duplicate = MultiShotConfig::try_from(MultiShotSource {
            spread_degrees: 2.0,
            allowed_patterns: vec!["first_2".to_owned(), "first_2".to_owned()],
            patterns,
        });
        assert!(
            duplicate
                .expect_err("duplicate allowed pattern accepted")
                .to_string()
                .contains("duplicate")
        );
    }

    #[test]
    fn multi_shot_offsets_are_centred_on_the_aim() {
        let column = 2.0_f32.to_radians();
        let row = 3.0_f32.to_radians();
        // Columns run screen-left to screen-right, which is decreasing yaw.
        assert_eq!(
            multi_shot(&["xox"]).expect("line stencil rejected").shots(),
            &[(column, 0.0), (0.0, 0.0), (-column, 0.0)]
        );
        // Rows run top-down from the marked aim shot.
        assert_eq!(
            multi_shot(&["o", "x"]).expect("column stencil rejected").shots(),
            &[(0.0, 0.0), (0.0, -row)]
        );
    }

    #[test]
    fn multi_shot_triangle_is_equilateral_at_root_three_rows() {
        let rows: Vec<String> = [".o.", "x.x"].map(str::to_owned).to_vec();
        let config =
            MultiShotConfig::from_stencil("multi_shot", 1.0, 3.0_f32.sqrt(), &rows).expect("triangle stencil rejected");
        let [top, left, right] = config.shots() else {
            panic!("triangle has {} shots", config.shots().len());
        };
        let side = |a: &(f32, f32), b: &(f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        assert!((side(top, left) - side(left, right)).abs() < 1e-6);
        assert!((side(top, right) - side(left, right)).abs() < 1e-6);
    }

    #[test]
    fn multi_shot_anchor_moves_the_aim() {
        let column = 2.0_f32.to_radians();
        let row = 3.0_f32.to_radians();
        // `o` fires from the aim; the other shots are measured from it.
        assert_eq!(
            multi_shot(&["x..", "..o"]).expect("anchored stencil rejected").shots(),
            &[(2.0 * column, row), (0.0, 0.0)]
        );
    }

    #[test]
    fn multi_shot_name_count_postfix_must_match() {
        let source = |name: &str| MultiShotSource {
            spread_degrees: 2.0,
            allowed_patterns: vec![name.to_owned()],
            patterns: HashMap::from([(
                name.to_owned(),
                MultiShotPattern {
                    column_scale: 1.0,
                    row_scale: 1.0,
                    stencil: vec!["xo".to_owned()],
                },
            )]),
        };
        assert!(MultiShotConfig::try_from(source("line_2")).is_ok());
        assert!(
            MultiShotConfig::try_from(source("line")).is_ok(),
            "no postfix, no claim"
        );
        let error = MultiShotConfig::try_from(source("line_3")).expect_err("wrong count accepted");
        assert!(error.to_string().contains("not the 3"));
    }

    #[test]
    fn multi_shot_pattern_is_validated() {
        let error = |pattern: &[&str]| multi_shot(pattern).expect_err("invalid stencil accepted").to_string();
        assert!(error(&["xx", "x"]).contains("width"));
        assert!(error(&["x-x"]).contains("'x', 'o' and '.'"));
        assert!(error(&["..."]).contains("center shot"));
        assert!(error(&["xxx"]).contains("center shot"));
        assert!(error(&["o.o"]).contains("only once"));
        assert!(error(&["oxxxxxxxxx"]).contains("max is"));
        assert!(multi_shot(&["x.x", ".o.", "x.x"]).is_ok());
    }
}
