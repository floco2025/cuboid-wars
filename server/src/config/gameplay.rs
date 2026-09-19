use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use bevy::prelude::Resource;
use map_core::merge_map_settings;
use serde::Deserialize;
use serde_json::Value;

use super::{
    actors::{ActorKindServerConfig, validate_actors},
    combat::CombatConfig,
    cycles::CyclesConfig,
    falling::FallDamageConfig,
    feed::FeedConfig,
    items::{PlacedItemsConfig, PowerUpsConfig},
    maps::{RandomItemsConfig, WeatherMode, validate_map_registry},
    quests::{Quest, validate_quests},
    respawn::RespawnConfig,
    scoring::ScoringConfig,
    validation::{deserialize_required_option, validate_covers_actor_kinds, validate_positive_finite},
    weapons::WeaponsConfig,
};
use common::{
    celestial::CelestialMapSettings,
    config::{
        ActorGameplayBootstrap, CharacterGameplayConfig, GameplayBootstrap, GameplayConfig, MapGeometryConfig,
        MapMovementConfig, MissilesGameplayBootstrap, NetworkConfig, PlayerGameplayBootstrap,
    },
    protocol::{ItemType, MapSettings, PortalMode, validate_texture_catalog},
};

// Every registered map's effective configuration, loaded from
// `gameplay.json` and each map's `settings.json`.
#[derive(Debug)]
pub struct GameplayCatalog {
    pub default_map: String,
    pub maps: HashMap<String, ServerGameplayConfig>,
}

impl GameplayCatalog {
    pub fn load_default() -> Result<Self> {
        Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/server/gameplay.json"
        )))
    }

    // The defaults are checked first with their own file named, so an error
    // that survives to a map is that map's own.
    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        let mut source: Value =
            serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        let file: GameplayFile =
            serde_json::from_value(source.clone()).with_context(|| format!("failed to parse {}", path.display()))?;
        validate_map_registry(file.maps.iter().map(String::as_str), &file.default_map)
            .with_context(|| format!("invalid map registry in {}", path.display()))?;
        file.tuning()
            .validate(&format!("{}: ", path.display()))
            .with_context(|| format!("invalid configuration loaded from {}", path.display()))?;
        strip_registry(&mut source);
        let directory = path.parent().context("gameplay configuration directory missing")?;
        let mut maps = HashMap::new();
        for name in file.maps {
            let settings_path = directory.join("maps").join(&name).join("settings.json");
            let text = fs::read_to_string(&settings_path)
                .with_context(|| format!("failed to read {}", settings_path.display()))?;
            let map: Value =
                serde_json::from_str(&text).with_context(|| format!("failed to parse {}", settings_path.display()))?;
            let config = ServerGameplayConfig::from_override(&name, &source, &map)
                .with_context(|| format!("failed to parse {}", settings_path.display()))?;
            config
                .validate(&format!("{}: ", settings_path.display()))
                .with_context(|| format!("invalid configuration loaded from {}", settings_path.display()))?;
            maps.insert(name, config);
        }
        Ok(Self {
            default_map: file.default_map,
            maps,
        })
    }

    // The named map's configuration, or the default map's.
    pub fn select(&self, name: Option<&str>) -> Result<ServerGameplayConfig> {
        let name = name.unwrap_or(&self.default_map);
        let Some(config) = self.maps.get(name) else {
            let mut known: Vec<&str> = self.maps.keys().map(String::as_str).collect();
            known.sort_unstable();
            bail!("unknown map {name:?} (available: {known:?})");
        };
        Ok(config.clone())
    }
}

// Leaves the defaults a map may override.
pub(crate) fn strip_registry(gameplay: &mut Value) {
    if let Some(object) = gameplay.as_object_mut() {
        object.remove("default_map");
        object.remove("maps");
    }
}

// The typed view of `gameplay.json`: the registry and every default section.
// Deserializing it rejects unknown and missing sections before any map is read.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GameplayFile {
    #[serde(default)]
    network: NetworkConfig,
    default_map: String,
    maps: Vec<String>,
    player: PlayerServerConfig,
    actors: HashMap<String, ActorKindServerConfig>,
    weapons: WeaponsConfig,
    scoring: ScoringConfig,
    cycles: CyclesConfig,
    feed: FeedConfig,
    celestial: CelestialMapSettings,
    geometry: MapGeometryConfig,
    movement: MapMovementConfig,
    #[expect(dead_code, reason = "deserialized so a malformed default is rejected")]
    portals: PortalMode,
    #[expect(dead_code, reason = "deserialized so a malformed default is rejected")]
    weather: WeatherMode,
    player_fall: FallDamageConfig,
    actor_fall: FallDamageConfig,
    combat: CombatConfig,
    #[expect(dead_code, reason = "deserialized so a malformed default is rejected")]
    respawn: RespawnConfig,
    power_ups: PowerUpsConfig,
}

impl GameplayFile {
    fn tuning(&self) -> Tuning<'_> {
        Tuning {
            network: &self.network,
            player: &self.player,
            actors: &self.actors,
            weapons: &self.weapons,
            scoring: &self.scoring,
            cycles: &self.cycles,
            feed: &self.feed,
            celestial: &self.celestial,
            geometry: &self.geometry,
            movement: &self.movement,
            player_fall: &self.player_fall,
            actor_fall: &self.actor_fall,
            combat: &self.combat,
            power_ups: &self.power_ups,
        }
    }
}

// The sections with value rules, shared by the defaults and every map's
// effective configuration; `prefix` names the file the values came from.
struct Tuning<'a> {
    network: &'a NetworkConfig,
    player: &'a PlayerServerConfig,
    actors: &'a HashMap<String, ActorKindServerConfig>,
    weapons: &'a WeaponsConfig,
    scoring: &'a ScoringConfig,
    cycles: &'a CyclesConfig,
    feed: &'a FeedConfig,
    celestial: &'a CelestialMapSettings,
    geometry: &'a MapGeometryConfig,
    movement: &'a MapMovementConfig,
    player_fall: &'a FallDamageConfig,
    actor_fall: &'a FallDamageConfig,
    combat: &'a CombatConfig,
    power_ups: &'a PowerUpsConfig,
}

impl Tuning<'_> {
    fn validate(&self, prefix: &str) -> Result<()> {
        self.network.validate()?;
        self.player.validate(&format!("{prefix}player"))?;
        validate_actors(self.actors, &format!("{prefix}actors"))?;
        self.weapons.validate(&format!("{prefix}weapons"))?;
        self.scoring.validate(self.actors, &format!("{prefix}scoring"))?;
        self.cycles.validate(&format!("{prefix}cycles"))?;
        self.feed.validate(self.actors, &format!("{prefix}feed"))?;
        self.celestial.validate(&format!("{prefix}celestial"))?;
        self.geometry.validate(&format!("{prefix}geometry"))?;
        let movement_path = format!("{prefix}movement");
        let movable_actors: HashMap<_, _> = self
            .actors
            .iter()
            .filter(|(_, actor)| !actor.character.immovable)
            .map(|(kind, actor)| (kind.clone(), actor))
            .collect();
        for kind in self.movement.actors.keys() {
            if self.actors.get(kind).is_some_and(|actor| actor.character.immovable) {
                bail!("{movement_path}.actors.{kind} must be omitted for an immovable actor");
            }
        }
        validate_covers_actor_kinds(
            self.movement.actors.keys(),
            &movable_actors,
            &format!("{movement_path}.actors"),
        )?;
        self.movement.validate(&movement_path)?;
        self.player_fall
            .validate(&format!("{prefix}player_fall"), self.movement.gravity)?;
        self.actor_fall
            .validate(&format!("{prefix}actor_fall"), self.movement.gravity)?;
        self.combat.validate(self.actors, &format!("{prefix}combat"))?;
        self.power_ups.validate(&format!("{prefix}power_ups"))
    }
}

// The effective configuration of one map: the `gameplay.json` defaults with
// the map's `settings.json` overrides applied, plus the map's own content.
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct ServerGameplayConfig {
    #[serde(skip)]
    pub map_name: String,
    #[serde(default)]
    pub network: NetworkConfig,
    pub player: PlayerServerConfig,
    pub actors: HashMap<String, ActorKindServerConfig>,
    pub weapons: WeaponsConfig,
    pub scoring: ScoringConfig,
    pub cycles: CyclesConfig,
    pub feed: FeedConfig,
    // The part that ships to clients in `SInit`.
    #[serde(flatten)]
    pub settings: MapSettings,
    pub player_fall: FallDamageConfig,
    // Ground actors only; flying actors never land.
    pub actor_fall: FallDamageConfig,
    pub combat: CombatConfig,
    // `None` = no random item spawning on this map.
    #[serde(deserialize_with = "deserialize_required_option")]
    pub random_items: Option<RandomItemsConfig>,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub placed_items: Option<PlacedItemsConfig>,
    pub power_ups: PowerUpsConfig,
    pub respawn: RespawnConfig,
    // A concrete state holds until an admin command; `auto` runs
    // `cycles.weather`. Mirrors `/weather rain|clear|auto`.
    pub weather: WeatherMode,
    pub quests: Vec<Quest>,
}

impl ServerGameplayConfig {
    // Builds a map's configuration from the defaults (`gameplay.json` without
    // its registry keys) and the map's `settings.json`.
    pub(crate) fn from_override(map_name: &str, defaults: &Value, map: &Value) -> Result<Self> {
        reject_pinned_actor_fields(map)?;
        let merged = merge_map_settings(defaults, map).context("invalid override")?;
        let mut config: Self = serde_json::from_value(merged)?;
        config.map_name = map_name.to_owned();
        Ok(config)
    }

    pub(crate) fn validate(&self, prefix: &str) -> Result<()> {
        self.tuning().validate(prefix)?;
        validate_texture_catalog(&self.settings.textures, &format!("{prefix}textures"))?;
        if let Some(random_items) = &self.random_items {
            random_items.validate(&format!("{prefix}random_items"))?;
            for id in random_items.weights.keys() {
                self.power_ups.validate_pickup(
                    ItemType::from_config_id(id).expect("validated random item type missing"),
                    &format!("{prefix}random_items.weights.{id}"),
                )?;
            }
        }
        if let Some(placed_items) = &self.placed_items {
            placed_items.validate(&format!("{prefix}placed_items"))?;
        }
        validate_quests(&self.quests, &self.actors, &format!("{prefix}quests"))
    }

    fn tuning(&self) -> Tuning<'_> {
        Tuning {
            network: &self.network,
            player: &self.player,
            actors: &self.actors,
            weapons: &self.weapons,
            scoring: &self.scoring,
            cycles: &self.cycles,
            feed: &self.feed,
            celestial: &self.settings.celestial,
            geometry: &self.settings.geometry,
            movement: &self.settings.movement,
            player_fall: &self.player_fall,
            actor_fall: &self.actor_fall,
            combat: &self.combat,
            power_ups: &self.power_ups,
        }
    }

    #[must_use]
    pub fn actor(&self, kind: &str) -> Option<&ActorKindServerConfig> {
        self.actors.get(kind)
    }

    #[must_use]
    pub fn expect_actor(&self, kind: &str) -> &ActorKindServerConfig {
        self.actor(kind)
            .expect("actor kind missing from server gameplay config")
    }

    #[must_use]
    pub fn gameplay_config(&self) -> GameplayConfig {
        GameplayConfig {
            player: self.player.gameplay.clone(),
            projectiles: self.weapons.projectiles.clone(),
            missiles: self.weapons.missiles.gameplay,
            portals: self.weapons.portals,
            actors: self
                .actors
                .iter()
                .map(|(kind, actor)| (kind.clone(), actor.character.clone()))
                .collect(),
        }
    }

    #[must_use]
    pub fn gameplay_bootstrap(&self) -> GameplayBootstrap {
        let combat = &self.combat;
        let mut actors: Vec<_> = self
            .actors
            .iter()
            .map(|(kind, actor)| {
                let health = combat
                    .health
                    .actors
                    .get(kind)
                    .expect("actor health missing after server config validation");
                let damage = combat
                    .damage
                    .actors
                    .get(kind)
                    .expect("actor damage missing after server config validation");
                (
                    kind.clone(),
                    ActorGameplayBootstrap {
                        gameplay: actor.character.clone(),
                        max_health: health.max,
                        death_blast_radius: damage.death_blast.radius,
                    },
                )
            })
            .collect();
        actors.sort_by(|a, b| a.0.cmp(&b.0));

        GameplayBootstrap {
            player: PlayerGameplayBootstrap {
                gameplay: self.player.gameplay.clone(),
                max_health: combat.health.player.max,
                death_blast_radius: combat.damage.player_blast.radius,
            },
            actors,
            projectiles: self.weapons.projectiles.clone(),
            missiles: MissilesGameplayBootstrap {
                gameplay: self.weapons.missiles.gameplay,
                blast_radius: combat.damage.missile_blast.radius,
            },
            portals: self.weapons.portals,
        }
    }
}

// `immovable` and `locomotion` decide which kinds `movement.actors` lists,
// and the merge cannot add or drop keys there, so a map cannot change them.
fn reject_pinned_actor_fields(map: &Value) -> Result<()> {
    let Some(actors) = map.get("actors").and_then(Value::as_object) else {
        return Ok(());
    };
    for (kind, actor) in actors {
        for field in ["immovable", "locomotion"] {
            if actor.get(field).is_some() {
                bail!("actors.{kind}.{field} cannot be overridden per map");
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerServerConfig {
    #[serde(flatten)]
    pub gameplay: CharacterGameplayConfig,
    pub respawn_secs: f32,
}

impl PlayerServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        self.gameplay.validate(path)?;
        validate_positive_finite(self.respawn_secs, &format!("{path}.respawn_secs"))
    }
}

#[cfg(test)]
#[path = "tests/gameplay.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/fixtures.rs"]
pub(crate) mod fixtures;
