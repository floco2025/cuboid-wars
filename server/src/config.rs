use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail};
use bevy::prelude::Resource;
use quinn::ServerConfig;
use serde::Deserialize;

use common::{
    config::{create_quinn_server_config, load_certs, load_private_key, resolve_actor_inheritance},
    protocol::QuestId,
};

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
    pub scoring: ScoringConfig,
    pub player: PlayerServerConfig,
    pub power_ups: PowerUpsConfig,
    pub cookies: CookiesConfig,
    pub keys: KeysConfig,
    pub quests: Vec<Quest>,
    pub actors: HashMap<String, ActorKindServerConfig>,
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
        let mut value: serde_json::Value =
            serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        resolve_actor_inheritance(&mut value, "actors")
            .with_context(|| format!("resolving actor inheritance in {}", path.display()))?;
        serde_json::from_value(value).with_context(|| format!("failed to deserialize {}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.version == SUPPORTED_VERSION,
            "unsupported server gameplay config version {} (expected {})",
            self.version,
            SUPPORTED_VERSION
        );
        self.player.validate("player")?;
        // No range check on scoring values — negative deltas are legal
        // (e.g., `player_death: -1` penalty), and so is zero. Just ensure
        // the section deserialized.
        let _ = &self.scoring;
        self.power_ups.validate("power_ups")?;
        self.cookies.validate("cookies")?;
        self.keys.validate("keys")?;
        validate_quests(&self.quests)?;
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
    pub fn actor(&self, kind: &str) -> Option<&ActorKindServerConfig> {
        self.actors.get(kind)
    }

    #[must_use]
    pub fn validated_actor(&self, kind: &str) -> &ActorKindServerConfig {
        self.actor(kind).expect("actor kind validated at startup")
    }
}

// Global scoring deltas. Negative values are legal (e.g., a death penalty)
// and the entire block is server-only state — clients read the resulting
// `score` field via `SSnapshot` and never need the per-event point values.
#[derive(Debug, Clone, Deserialize)]
pub struct ScoringConfig {
    pub player_kill: i32,
    pub player_death: i32,
    pub cookie: i32,
    pub actor_hit: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerServerConfig {
    // Damage the player takes from a single incoming projectile.
    pub projectile_damage_taken: f32,
    // Debug toggle: when true, no damage source can take a player to zero
    // health. Projectile hits, actor explosions, and fall damage are all
    // skipped. Leave `false` for normal play.
    #[serde(default)]
    pub invincible: bool,
    pub fall_damage: FallDamageConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct FallDamageConfig {
    // Below this fall distance (meters), landing does no damage.
    pub safe_fall_distance: f32,
    // At this fall distance, landing deals `max_health` damage (lethal).
    // Damage lerps linearly between the two endpoints and clamps past
    // `lethal_fall_distance`.
    pub lethal_fall_distance: f32,
}

impl PlayerServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.projectile_damage_taken, &format!("{path}.projectile_damage_taken"))?;
        self.fall_damage.validate(&format!("{path}.fall_damage"))
    }
}

impl FallDamageConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.safe_fall_distance, &format!("{path}.safe_fall_distance"))?;
        validate_non_negative_finite(self.lethal_fall_distance, &format!("{path}.lethal_fall_distance"))?;
        if self.safe_fall_distance >= self.lethal_fall_distance {
            bail!(
                "{path}.safe_fall_distance ({}) must be < lethal_fall_distance ({})",
                self.safe_fall_distance,
                self.lethal_fall_distance
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PowerUpsConfig {
    // Target/cap for active power-ups in the world. The spawner paces
    // spawns to maintain this many and refuses to exceed it. Capped at
    // the number of eligible floor cells so tiny test maps degrade.
    pub max_number: usize,
    // How long an uncollected power-up sits in the world before being
    // removed. Cookies and keys use `respawn_secs` instead — they're
    // hidden after collection and re-shown, not despawned.
    pub despawn_secs: f32,
    pub speed_duration_secs: f32,
    pub multi_shot_duration_secs: f32,
    pub phasing_duration_secs: f32,
    pub anti_gravity_duration_secs: f32,
    // Fraction of max health restored by a single Health Potion pickup.
    // 0.0 < value <= 1.0 (1.0 = full heal). No duration — instant effect.
    pub health_potion_heal_percent: f32,
}

impl PowerUpsConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.despawn_secs, &format!("{path}.despawn_secs"))?;
        validate_non_negative_finite(self.speed_duration_secs, &format!("{path}.speed_duration_secs"))?;
        validate_non_negative_finite(
            self.multi_shot_duration_secs,
            &format!("{path}.multi_shot_duration_secs"),
        )?;
        validate_non_negative_finite(self.phasing_duration_secs, &format!("{path}.phasing_duration_secs"))?;
        validate_non_negative_finite(
            self.anti_gravity_duration_secs,
            &format!("{path}.anti_gravity_duration_secs"),
        )?;
        if !(self.health_potion_heal_percent > 0.0 && self.health_potion_heal_percent <= 1.0) {
            bail!(
                "{path}.health_potion_heal_percent must be in (0.0, 1.0], got {}",
                self.health_potion_heal_percent
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CookiesConfig {
    pub spawning_enabled: bool,
    pub respawn_secs: f32,
}

impl CookiesConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.respawn_secs, &format!("{path}.respawn_secs"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeysConfig {
    pub respawn_secs: f32,
}

impl KeysConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.respawn_secs, &format!("{path}.respawn_secs"))
    }
}

// One quest the server auto-assigns to every player at login. Server-only:
// the wire ships only the per-quest `announcement_text` / `achieved_text`
// strings inline on `SQuestNew` / `SQuestAchieved`; clients never see the
// kind or threshold.
#[derive(Debug, Clone, Deserialize)]
pub struct Quest {
    pub id: QuestId,
    pub kind: QuestKind,
    pub threshold: u32,
    pub announcement_text: String,
    pub achieved_text: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QuestKind {
    Cookies,
}

fn validate_quests(quests: &[Quest]) -> Result<()> {
    if quests.is_empty() {
        bail!("quests list must contain at least one quest");
    }
    let mut seen_ids: HashSet<&QuestId> = HashSet::with_capacity(quests.len());
    for (idx, quest) in quests.iter().enumerate() {
        let path = format!("quests[{idx}]");
        if quest.id.0.is_empty() {
            bail!("{path}.id must not be empty");
        }
        if !seen_ids.insert(&quest.id) {
            bail!("{path}.id `{}` is duplicated", quest.id.0);
        }
        if quest.threshold == 0 {
            bail!("{path}.threshold must be > 0");
        }
        if quest.announcement_text.is_empty() {
            bail!("{path}.announcement_text must not be empty");
        }
        if quest.achieved_text.is_empty() {
            bail!("{path}.achieved_text must not be empty");
        }
    }
    Ok(())
}

// Server-side per-actor-kind tuning. Fields are grouped by concern so the
// JSON reads as a self-documenting outline; cross-cutting numbers (combat,
// senses, navigation, patrol, chase, respawn) don't get jumbled into one
// flat list.
#[derive(Debug, Clone, Deserialize)]
pub struct ActorKindServerConfig {
    pub respawn: ActorRespawnConfig,
    pub combat: ActorCombatConfig,
    pub senses: ActorSensesConfig,
    pub patrol: ActorPatrolConfig,
    pub chase: ActorChaseConfig,
    pub navigation: ActorNavigationConfig,
}

impl ActorKindServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        self.respawn.validate(&format!("{path}.respawn"))?;
        self.combat.validate(&format!("{path}.combat"))?;
        self.senses.validate(&format!("{path}.senses"))?;
        self.patrol.validate(&format!("{path}.patrol"))?;
        self.chase.validate(&format!("{path}.chase"))?;
        self.navigation.validate(&format!("{path}.navigation"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorRespawnConfig {
    // When false, the zone fills `count` actors at startup and is never
    // refilled. When true (default), deaths trigger a respawn after the
    // configured delay.
    #[serde(default = "default_respawn_enabled")]
    pub enabled: bool,
    // Delay between an actor's death and its replacement appearing. Only
    // applies when `enabled` is true. 0.0 means immediate respawn.
    #[serde(default)]
    pub delay_secs: f32,
}

const fn default_respawn_enabled() -> bool {
    true
}

impl ActorRespawnConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.delay_secs, &format!("{path}.delay_secs"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorCombatConfig {
    // Damage this actor takes from a single player projectile hit. Per-kind
    // so a tougher actor can need more shots without changing the projectile.
    pub projectile_damage_taken: f32,
    // Distance at which contact with a player triggers the actor's explosion.
    pub contact_explosion_distance: f32,
    pub explosion: ActorExplosionDamageConfig,
    // Bonus added to the killer's score on the lethal hit, on top of
    // `scoring.actor_hit` which fires per hit. Tougher actors should be
    // worth more — set higher for sentry, lower for mine_1.
    #[serde(default)]
    pub score_reward_on_kill: i32,
}

impl ActorCombatConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.projectile_damage_taken, &format!("{path}.projectile_damage_taken"))?;
        validate_non_negative_finite(
            self.contact_explosion_distance,
            &format!("{path}.contact_explosion_distance"),
        )?;
        self.explosion.validate(&format!("{path}.explosion"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorSensesConfig {
    // Range gate is a box, not a sphere: a player passes when within
    // `horizontal_vision_range` on the xz plane AND `vertical_vision_range`
    // on y. LOS still has the final say.
    pub horizontal_vision_range: f32,
    pub vertical_vision_range: f32,
    // Delay after the actor reaches the last known player position before it
    // may acquire a visible player as a fresh chase target again.
    #[serde(default)]
    pub chase_reacquire_cooldown_secs: f32,
}

impl ActorSensesConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.horizontal_vision_range, &format!("{path}.horizontal_vision_range"))?;
        validate_positive_finite(self.vertical_vision_range, &format!("{path}.vertical_vision_range"))?;
        validate_non_negative_finite(
            self.chase_reacquire_cooldown_secs,
            &format!("{path}.chase_reacquire_cooldown_secs"),
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorPatrolConfig {
    // Maximum xz-distance (meters) from the spawn zone's nearest edge a
    // patrolling actor may stray before it breaks off and walks home.
    // Inside the zone counts as 0.
    pub leash: f32,
    pub min_direction_secs: f32,
    pub max_direction_secs: f32,
    pub idle_probability: f32,
}

impl ActorPatrolConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.leash, &format!("{path}.leash"))?;
        validate_positive_finite(self.min_direction_secs, &format!("{path}.min_direction_secs"))?;
        validate_positive_finite(self.max_direction_secs, &format!("{path}.max_direction_secs"))?;
        if self.min_direction_secs > self.max_direction_secs {
            bail!("{path}.min_direction_secs must be <= {path}.max_direction_secs");
        }
        validate_probability(self.idle_probability, &format!("{path}.idle_probability"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorChaseConfig {
    // Maximum xz-distance (meters) from the spawn zone's nearest edge a
    // chasing actor may stray before it breaks off the chase (triggering
    // `senses.chase_reacquire_cooldown_secs`) and walks home. Typically larger
    // than `patrol.leash` so a predator can pursue a fleeing player past
    // its normal roam.
    pub leash: f32,
}

impl ActorChaseConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.leash, &format!("{path}.leash"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorNavigationConfig {
    pub path_clear_lookahead_secs: f32,
    pub go_to_reached_distance: f32,
}

impl ActorNavigationConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(
            self.path_clear_lookahead_secs,
            &format!("{path}.path_clear_lookahead_secs"),
        )?;
        validate_positive_finite(self.go_to_reached_distance, &format!("{path}.go_to_reached_distance"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActorExplosionDamageConfig {
    pub radius: f32,
    pub player_max_damage: f32,
    pub actor_max_damage: f32,
}

impl ActorExplosionDamageConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.radius, &format!("{path}.radius"))?;
        validate_non_negative_finite(self.player_max_damage, &format!("{path}.player_max_damage"))?;
        validate_non_negative_finite(self.actor_max_damage, &format!("{path}.actor_max_damage"))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_quest(id: &str, threshold: u32) -> Quest {
        Quest {
            id: QuestId(id.to_owned()),
            kind: QuestKind::Cookies,
            threshold,
            announcement_text: "go".to_owned(),
            achieved_text: "done".to_owned(),
        }
    }

    #[test]
    fn validate_quests_accepts_single_valid_entry() {
        validate_quests(&[ok_quest("a", 10)]).expect("valid quest should pass");
    }

    #[test]
    fn validate_quests_rejects_empty_list() {
        let err = validate_quests(&[]).expect_err("empty list must be rejected");
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn validate_quests_rejects_duplicate_ids() {
        let err = validate_quests(&[ok_quest("dup", 5), ok_quest("dup", 7)]).expect_err("dup ids must be rejected");
        assert!(err.to_string().contains("duplicated"));
    }

    #[test]
    fn validate_quests_rejects_zero_threshold() {
        let err = validate_quests(&[ok_quest("z", 0)]).expect_err("zero threshold must be rejected");
        assert!(err.to_string().contains("threshold"));
    }

    #[test]
    fn validate_quests_rejects_empty_announcement_text() {
        let mut quest = ok_quest("a", 1);
        quest.announcement_text = String::new();
        let err = validate_quests(&[quest]).expect_err("empty announcement_text must be rejected");
        assert!(err.to_string().contains("announcement_text"));
    }

    #[test]
    fn validate_quests_rejects_empty_achieved_text() {
        let mut quest = ok_quest("a", 1);
        quest.achieved_text = String::new();
        let err = validate_quests(&[quest]).expect_err("empty achieved_text must be rejected");
        assert!(err.to_string().contains("achieved_text"));
    }
}
