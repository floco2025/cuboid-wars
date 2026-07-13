use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail};
use bevy::prelude::Resource;
use serde::Deserialize;

use super::actors::{ActorKindServerConfig, ExplosionDamageConfig};
use super::validation::{validate_non_negative_finite, validate_positive_finite, validate_probability};
use common::{
    config::resolve_actor_inheritance,
    protocol::{ItemType, MapSettings, QuestId},
};

const SUPPORTED_VERSION: u32 = 1;

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct ServerGameplayConfig {
    pub version: u32,
    // Named-map registry: each entry's map geometry lives at
    // `config/server/maps/<name>.json`.
    pub maps: HashMap<String, MapServerConfig>,
    pub default_map: String,
    pub scoring: ScoringConfig,
    pub player: PlayerServerConfig,
    pub projectile: ProjectileConfig,
    pub power_ups: PowerUpsConfig,
    pub placed_items: PlacedItemsConfig,
    pub quests: Vec<Quest>,
    pub actors: HashMap<String, ActorKindServerConfig>,
}

// Server-side wrapper around the wire `MapSettings`: the flattened settings
// ship to clients in `SInit`, while `random_items` stays server-only.
#[derive(Debug, Clone, Deserialize)]
pub struct MapServerConfig {
    #[serde(flatten)]
    pub settings: MapSettings,
    // `None` = no random item spawning on this map.
    #[serde(default)]
    pub random_items: Option<RandomItemsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RandomItemsConfig {
    // `ItemType` config ids. Keys are rejected — they're parameterized by
    // barrier kind and must be placed in the map's `items` list.
    pub types: Vec<String>,
    // Target/cap for active random items in the world. The spawner paces
    // spawns to maintain this many and refuses to exceed it. Capped at the
    // number of eligible floor cells so tiny test maps degrade.
    pub max_number: usize,
    // How long an uncollected random item sits in the world before being
    // removed. Placed items use `placed_items.respawn_secs` instead.
    pub despawn_secs: f32,
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
        validate_maps(&self.maps, &self.default_map)?;
        self.player.validate("player")?;
        self.projectile.validate("projectile")?;
        // No range check on scoring values — negative deltas are legal
        // (e.g., `player_death: -1` penalty), and so is zero. Just ensure
        // the section deserialized.
        let _ = &self.scoring;
        self.power_ups.validate("power_ups")?;
        self.placed_items.validate("placed_items")?;
        validate_quests(&self.quests, &self.actors)?;
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
        self.actor(kind)
            .expect("actor kind missing from server gameplay config")
    }
}

// One projectile, one raw damage number. What a victim actually receives is
// `damage * (1 - armor)` — per-victim toughness lives on `armor`, not here.
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectileConfig {
    pub damage: f32,
}

impl ProjectileConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.damage, &format!("{path}.damage"))
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
    // Fraction of incoming damage players block (0.0 = unarmored, 0.9 =
    // takes 10%). Applies to projectile hits and explosion blasts; fall
    // damage ignores armor (it's defined as a fraction of max health by
    // fall distance).
    pub armor: f32,
    // Debug toggle: when true, no damage source can take a player to zero
    // health. Projectile hits, actor explosions, and fall damage are all
    // skipped. Leave `false` for normal play.
    #[serde(default)]
    pub invincible: bool,
    pub fall_damage: FallDamageConfig,
    // Blast dealt by a dying player, same shape as the per-actor-kind
    // `combat.explosion` — standing next to your victim is now a mistake.
    pub explosion: ExplosionDamageConfig,
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
        validate_probability(self.armor, &format!("{path}.armor"))?;
        self.fall_damage.validate(&format!("{path}.fall_damage"))?;
        self.explosion.validate(&format!("{path}.explosion"))
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

// Power-up effect tuning. Spawning lives elsewhere: random pools per map in
// `maps.<name>.random_items`, placed respawns in `placed_items`.
#[derive(Debug, Clone, Deserialize)]
pub struct PowerUpsConfig {
    pub speed_duration_secs: f32,
    pub multi_shot_duration_secs: f32,
    pub phasing_duration_secs: f32,
    pub low_gravity_duration_secs: f32,
    // Fraction of max health restored by a single Health Potion pickup.
    // 0.0 < value <= 1.0 (1.0 = full heal). No duration — instant effect.
    pub health_potion_heal_fraction: f32,
}

impl PowerUpsConfig {
    // Per-`PowerUpKind` duration, sourced from the named JSON fields. Lets
    // the rest of the server look up durations by enum variant without
    // each call site enumerating the four matches.
    #[must_use]
    pub const fn duration_secs(&self, kind: common::protocol::PowerUpKind) -> f32 {
        use common::protocol::PowerUpKind as K;
        match kind {
            K::Speed => self.speed_duration_secs,
            K::MultiShot => self.multi_shot_duration_secs,
            K::Phasing => self.phasing_duration_secs,
            K::LowGravity => self.low_gravity_duration_secs,
        }
    }

    fn validate(&self, path: &str) -> Result<()> {
        validate_non_negative_finite(self.speed_duration_secs, &format!("{path}.speed_duration_secs"))?;
        validate_non_negative_finite(
            self.multi_shot_duration_secs,
            &format!("{path}.multi_shot_duration_secs"),
        )?;
        validate_non_negative_finite(self.phasing_duration_secs, &format!("{path}.phasing_duration_secs"))?;
        validate_non_negative_finite(
            self.low_gravity_duration_secs,
            &format!("{path}.low_gravity_duration_secs"),
        )?;
        if !(self.health_potion_heal_fraction > 0.0 && self.health_potion_heal_fraction <= 1.0) {
            bail!(
                "{path}.health_potion_heal_fraction must be in (0.0, 1.0], got {}",
                self.health_potion_heal_fraction
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlacedItemsConfig {
    pub respawn_secs: PlacedItemRespawnSecs,
}

// How long a placed item stays hidden after pickup before reappearing at
// its cell. One value per `ItemType` config id; 0.0 = instant reappear.
#[derive(Debug, Clone, Deserialize)]
pub struct PlacedItemRespawnSecs {
    pub speed: f32,
    pub multi_shot: f32,
    pub phasing: f32,
    pub low_gravity: f32,
    pub health_potion: f32,
    pub cookie: f32,
    pub key: f32,
}

impl PlacedItemsConfig {
    #[must_use]
    pub const fn respawn_secs_for(&self, item_type: ItemType) -> f32 {
        let secs = &self.respawn_secs;
        match item_type {
            ItemType::SpeedPowerUp => secs.speed,
            ItemType::MultiShotPowerUp => secs.multi_shot,
            ItemType::PhasingPowerUp => secs.phasing,
            ItemType::LowGravityPowerUp => secs.low_gravity,
            ItemType::HealthPotion => secs.health_potion,
            ItemType::Cookie => secs.cookie,
            ItemType::Key(_) => secs.key,
        }
    }

    fn validate(&self, path: &str) -> Result<()> {
        let secs = &self.respawn_secs;
        for (value, name) in [
            (secs.speed, "speed"),
            (secs.multi_shot, "multi_shot"),
            (secs.phasing, "phasing"),
            (secs.low_gravity, "low_gravity"),
            (secs.health_potion, "health_potion"),
            (secs.cookie, "cookie"),
            (secs.key, "key"),
        ] {
            validate_non_negative_finite(value, &format!("{path}.respawn_secs.{name}"))?;
        }
        Ok(())
    }
}

// One quest the server assigns to a player. Server-only: the wire ships only
// the per-quest `title` / `description` / `completed_text` strings (plus
// progress/threshold numbers) on the quest messages; clients never see the
// kind or the `actor_kind` filter.
#[derive(Debug, Clone, Deserialize)]
pub struct Quest {
    pub id: QuestId,
    pub kind: QuestKind,
    // For `ActorKills`: when `Some`, only kills of that actor kind count;
    // `None` counts any actor. Ignored by other kinds.
    #[serde(default)]
    pub actor_kind: Option<String>,
    pub threshold: u32,
    // Short label for the quest panel.
    pub title: String,
    // Longer body shown (with the title) in the announcement banner.
    pub description: String,
    pub completed_text: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QuestKind {
    Cookies,
    ActorKills,
}

fn validate_maps(maps: &HashMap<String, MapServerConfig>, default_map: &str) -> Result<()> {
    if maps.is_empty() {
        bail!("maps must define at least one map");
    }
    for (name, entry) in maps {
        // Map names become file names (`config/server/maps/<name>.json`), so
        // reject anything that could traverse paths.
        if name.is_empty() {
            bail!("map name must not be empty");
        }
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            bail!("map name `{name}` must contain only ASCII letters, digits, `_`, or `-`");
        }
        let path = format!("maps.{name}");
        if entry.settings.skybox.is_empty() {
            bail!("{path}.skybox must not be empty");
        }
        if !entry.settings.gravity.is_finite() || entry.settings.gravity <= 0.0 {
            bail!("{path}.gravity must be > 0");
        }
        if !entry.settings.low_gravity.is_finite() || entry.settings.low_gravity < 0.0 {
            bail!("{path}.low_gravity must be >= 0");
        }
        if let Some(random_items) = &entry.random_items {
            random_items.validate(&format!("{path}.random_items"))?;
        }
    }
    if !maps.contains_key(default_map) {
        let mut known: Vec<&str> = maps.keys().map(String::as_str).collect();
        known.sort_unstable();
        bail!("default_map `{default_map}` is not a defined map (defined: {known:?})");
    }
    Ok(())
}

impl RandomItemsConfig {
    fn validate(&self, path: &str) -> Result<()> {
        if self.types.is_empty() {
            bail!("{path}.types must not be empty");
        }
        let mut seen: HashSet<&str> = HashSet::with_capacity(self.types.len());
        for ty in &self.types {
            if ty == ItemType::KEY_CONFIG_ID {
                bail!(
                    "{path}.types: keys are parameterized by barrier kind and cannot spawn randomly; place them in the map's `items` list"
                );
            }
            if ItemType::from_config_id(ty).is_none() {
                bail!("{path}.types contains unknown item type {ty:?}");
            }
            if !seen.insert(ty.as_str()) {
                bail!("{path}.types contains duplicate {ty:?}");
            }
        }
        if self.max_number == 0 {
            bail!("{path}.max_number must be >= 1");
        }
        validate_positive_finite(self.despawn_secs, &format!("{path}.despawn_secs"))
    }
}

fn validate_quests(quests: &[Quest], actors: &HashMap<String, ActorKindServerConfig>) -> Result<()> {
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
        if quest.title.is_empty() {
            bail!("{path}.title must not be empty");
        }
        if quest.description.is_empty() {
            bail!("{path}.description must not be empty");
        }
        if quest.completed_text.is_empty() {
            bail!("{path}.completed_text must not be empty");
        }
        match (quest.kind, &quest.actor_kind) {
            (QuestKind::ActorKills, Some(kind)) if !actors.contains_key(kind) => {
                bail!("{path}.actor_kind `{kind}` is not a known actor kind");
            }
            (kind, Some(_)) if kind != QuestKind::ActorKills => {
                bail!("{path}.actor_kind is only valid on an actor_kills quest");
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_quest(id: &str, threshold: u32) -> Quest {
        Quest {
            id: QuestId(id.to_owned()),
            kind: QuestKind::Cookies,
            actor_kind: None,
            threshold,
            title: "Go".to_owned(),
            description: "go do the thing".to_owned(),
            completed_text: "done".to_owned(),
        }
    }

    fn no_actors() -> HashMap<String, ActorKindServerConfig> {
        HashMap::new()
    }

    fn default_actors() -> HashMap<String, ActorKindServerConfig> {
        ServerGameplayConfig::load_default()
            .expect("default server gameplay config should load")
            .actors
    }

    fn ok_map_entry() -> MapServerConfig {
        MapServerConfig {
            settings: MapSettings {
                skybox: "cloudy_day".to_owned(),
                gravity: 25.0,
                low_gravity: 5.0,
            },
            random_items: None,
        }
    }

    fn ok_random_items(types: &[&str]) -> RandomItemsConfig {
        RandomItemsConfig {
            types: types.iter().map(|&t| t.to_owned()).collect(),
            max_number: 30,
            despawn_secs: 60.0,
        }
    }

    fn one_map(name: &str) -> HashMap<String, MapServerConfig> {
        HashMap::from([(name.to_owned(), ok_map_entry())])
    }

    fn one_map_with_random_items(name: &str, random_items: RandomItemsConfig) -> HashMap<String, MapServerConfig> {
        let mut maps = one_map(name);
        maps.get_mut(name).expect("map entry missing").random_items = Some(random_items);
        maps
    }

    #[test]
    fn validate_maps_accepts_single_valid_entry() {
        validate_maps(&one_map("hotel"), "hotel").expect("valid map registry should pass");
    }

    #[test]
    fn validate_maps_rejects_empty_registry() {
        let err = validate_maps(&HashMap::new(), "hotel").expect_err("empty registry must be rejected");
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn validate_maps_rejects_unknown_default_map() {
        let err = validate_maps(&one_map("hotel"), "lobby").expect_err("unknown default must be rejected");
        assert!(err.to_string().contains("default_map"));
    }

    #[test]
    fn validate_maps_rejects_path_unsafe_name() {
        let err = validate_maps(&one_map("../hotel"), "../hotel").expect_err("path chars must be rejected");
        assert!(err.to_string().contains("ASCII"));
    }

    #[test]
    fn validate_maps_rejects_non_positive_gravity() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel").expect("hotel entry missing").settings.gravity = 0.0;
        let err = validate_maps(&maps, "hotel").expect_err("zero gravity must be rejected");
        assert!(err.to_string().contains("gravity"));
    }

    #[test]
    fn validate_maps_rejects_negative_low_gravity() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel").expect("hotel entry missing").settings.low_gravity = -1.0;
        let err = validate_maps(&maps, "hotel").expect_err("negative low_gravity must be rejected");
        assert!(err.to_string().contains("low_gravity"));
    }

    #[test]
    fn validate_maps_rejects_empty_skybox() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel").expect("hotel entry missing").settings.skybox = String::new();
        let err = validate_maps(&maps, "hotel").expect_err("empty skybox must be rejected");
        assert!(err.to_string().contains("skybox"));
    }

    #[test]
    fn validate_maps_accepts_map_without_random_items() {
        validate_maps(&one_map("hotel"), "hotel").expect("map without random_items should pass");
    }

    #[test]
    fn validate_maps_accepts_valid_random_items() {
        let maps = one_map_with_random_items("hotel", ok_random_items(&["speed", "cookie"]));
        validate_maps(&maps, "hotel").expect("valid random_items should pass");
    }

    #[test]
    fn validate_maps_rejects_key_in_random_pool() {
        let maps = one_map_with_random_items("hotel", ok_random_items(&["speed", "key"]));
        let err = validate_maps(&maps, "hotel").expect_err("key in random pool must be rejected");
        assert!(err.to_string().contains("barrier kind"));
    }

    #[test]
    fn validate_maps_rejects_unknown_random_item_type() {
        let maps = one_map_with_random_items("hotel", ok_random_items(&["banana"]));
        let err = validate_maps(&maps, "hotel").expect_err("unknown type must be rejected");
        assert!(err.to_string().contains("unknown item type"));
    }

    #[test]
    fn validate_maps_rejects_duplicate_random_item_types() {
        let maps = one_map_with_random_items("hotel", ok_random_items(&["speed", "speed"]));
        let err = validate_maps(&maps, "hotel").expect_err("duplicate type must be rejected");
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn validate_maps_rejects_empty_random_item_types() {
        let maps = one_map_with_random_items("hotel", ok_random_items(&[]));
        let err = validate_maps(&maps, "hotel").expect_err("empty pool must be rejected");
        assert!(err.to_string().contains("types"));
    }

    #[test]
    fn validate_maps_rejects_zero_random_item_max_number() {
        let mut random_items = ok_random_items(&["speed"]);
        random_items.max_number = 0;
        let maps = one_map_with_random_items("hotel", random_items);
        let err = validate_maps(&maps, "hotel").expect_err("zero max_number must be rejected");
        assert!(err.to_string().contains("max_number"));
    }

    #[test]
    fn placed_item_respawn_secs_matches_item_type() {
        let config = PlacedItemsConfig {
            respawn_secs: PlacedItemRespawnSecs {
                speed: 1.0,
                multi_shot: 2.0,
                phasing: 3.0,
                low_gravity: 4.0,
                health_potion: 5.0,
                cookie: 6.0,
                key: 7.0,
            },
        };
        assert_eq!(config.respawn_secs_for(ItemType::SpeedPowerUp), 1.0);
        assert_eq!(config.respawn_secs_for(ItemType::MultiShotPowerUp), 2.0);
        assert_eq!(config.respawn_secs_for(ItemType::PhasingPowerUp), 3.0);
        assert_eq!(config.respawn_secs_for(ItemType::LowGravityPowerUp), 4.0);
        assert_eq!(config.respawn_secs_for(ItemType::HealthPotion), 5.0);
        assert_eq!(config.respawn_secs_for(ItemType::Cookie), 6.0);
        assert_eq!(
            config.respawn_secs_for(ItemType::Key(common::protocol::BarrierKindId(0))),
            7.0
        );
    }

    #[test]
    fn validate_quests_accepts_single_valid_entry() {
        validate_quests(&[ok_quest("a", 10)], &no_actors()).expect("valid quest should pass");
    }

    #[test]
    fn validate_quests_rejects_empty_list() {
        let err = validate_quests(&[], &no_actors()).expect_err("empty list must be rejected");
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn validate_quests_rejects_duplicate_ids() {
        let err = validate_quests(&[ok_quest("dup", 5), ok_quest("dup", 7)], &no_actors())
            .expect_err("dup ids must be rejected");
        assert!(err.to_string().contains("duplicated"));
    }

    #[test]
    fn validate_quests_rejects_zero_threshold() {
        let err = validate_quests(&[ok_quest("z", 0)], &no_actors()).expect_err("zero threshold must be rejected");
        assert!(err.to_string().contains("threshold"));
    }

    #[test]
    fn validate_quests_rejects_empty_title() {
        let mut quest = ok_quest("a", 1);
        quest.title = String::new();
        let err = validate_quests(&[quest], &no_actors()).expect_err("empty title must be rejected");
        assert!(err.to_string().contains("title"));
    }

    #[test]
    fn validate_quests_rejects_empty_description() {
        let mut quest = ok_quest("a", 1);
        quest.description = String::new();
        let err = validate_quests(&[quest], &no_actors()).expect_err("empty description must be rejected");
        assert!(err.to_string().contains("description"));
    }

    #[test]
    fn validate_quests_rejects_empty_completed_text() {
        let mut quest = ok_quest("a", 1);
        quest.completed_text = String::new();
        let err = validate_quests(&[quest], &no_actors()).expect_err("empty completed_text must be rejected");
        assert!(err.to_string().contains("completed_text"));
    }

    #[test]
    fn validate_quests_accepts_actor_kills_with_known_actor_kind() {
        let mut quest = ok_quest("hunt", 4);
        quest.kind = QuestKind::ActorKills;
        quest.actor_kind = Some("sentry".to_owned());
        validate_quests(&[quest], &default_actors()).expect("known actor kind should pass");
    }

    #[test]
    fn validate_quests_rejects_actor_kills_with_unknown_actor_kind() {
        let mut quest = ok_quest("hunt", 4);
        quest.kind = QuestKind::ActorKills;
        quest.actor_kind = Some("dragon".to_owned());
        let err = validate_quests(&[quest], &default_actors()).expect_err("unknown actor kind must be rejected");
        assert!(err.to_string().contains("actor_kind"));
    }

    #[test]
    fn validate_quests_rejects_actor_kind_on_non_actor_kills_quest() {
        let mut quest = ok_quest("oops", 4);
        quest.actor_kind = Some("sentry".to_owned());
        let err = validate_quests(&[quest], &default_actors()).expect_err("actor_kind on cookies must be rejected");
        assert!(err.to_string().contains("actor_kind"));
    }
}
