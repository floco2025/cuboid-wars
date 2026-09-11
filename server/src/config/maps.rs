use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::{
    actors::ActorKindServerConfig,
    falling::FallDamageConfig,
    items::{PlacedItemsConfig, PowerUpsConfig},
    quests::{Quest, validate_quests},
    respawn::RespawnConfig,
    validation::{deserialize_required_option, validate_covers_actor_kinds, validate_positive_finite},
};
use common::protocol::{ItemType, MapSettings, validate_texture_catalog};

// Server-side wrapper around the wire `MapSettings`: the flattened settings
// ship to clients in `SInit`, while the rest stays server-only.
#[derive(Debug, Clone, Deserialize)]
pub struct MapServerConfig {
    #[serde(flatten)]
    pub settings: MapSettings,
    pub player_fall: FallDamageConfig,
    // `None` = no random item spawning on this map.
    #[serde(deserialize_with = "deserialize_required_option")]
    pub random_items: Option<RandomItemsConfig>,
    pub placed_items: PlacedItemsConfig,
    pub power_ups: PowerUpsConfig,
    pub respawn: RespawnConfig,
    // A concrete state holds until an admin command; `auto` runs the
    // global `cycles.weather`. Mirrors `/weather rain|clear|auto`.
    pub weather: WeatherMode,
    // A concrete look holds until an admin command; `auto` runs the
    // global `cycles.lighting`. Mirrors `/light bright|dim|dark|auto`.
    pub lighting: LightingMode,
    pub quests: Vec<Quest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherMode {
    Clear,
    Rain,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightingMode {
    Bright,
    Dim,
    Dark,
    Auto,
}

impl LightingMode {
    // The preset a concrete mode holds; `None` = cycle-driven.
    #[must_use]
    pub const fn preset(self) -> Option<&'static str> {
        match self {
            Self::Bright => Some("bright"),
            Self::Dim => Some("dim"),
            Self::Dark => Some("dark"),
            Self::Auto => None,
        }
    }
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
    // removed. Placed items use the map's `placed_items.respawn_secs` instead.
    pub despawn_secs: f32,
}

// Registry map names become file names, so nothing that could traverse paths passes.
#[must_use]
pub(crate) fn is_valid_map_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub(super) fn validate_maps(
    maps: &HashMap<String, MapServerConfig>,
    default_map: &str,
    actors: &HashMap<String, ActorKindServerConfig>,
    directory: &Path,
) -> Result<()> {
    validate_map_registry(maps.keys().map(String::as_str), default_map)?;
    let movable_actors: HashMap<_, _> = actors
        .iter()
        .filter(|(_, actor)| !actor.character.immovable)
        .map(|(kind, actor)| (kind.clone(), actor))
        .collect();
    for (name, entry) in maps {
        let path = format!("{}:", directory.join(name).join("settings.json").display());
        if entry.settings.skybox.is_empty() {
            bail!("{path} skybox must not be empty");
        }
        entry
            .settings
            .kind_tables()
            .with_context(|| format!("invalid {path} barrier_kinds or bridge_kinds"))?;
        validate_texture_catalog(&entry.settings.textures, &format!("{path} textures"))?;
        entry.settings.geometry.validate(&format!("{path} geometry"))?;
        let movement_path = format!("{path} movement");
        let movement = &entry.settings.movement;
        for kind in movement.actors.keys() {
            if actors.get(kind).is_some_and(|actor| actor.character.immovable) {
                bail!("{movement_path}.actors.{kind} must be omitted for an immovable actor");
            }
        }
        validate_covers_actor_kinds(
            movement.actors.keys(),
            &movable_actors,
            &format!("{movement_path}.actors"),
        )?;
        movement.validate(&movement_path)?;
        entry.player_fall.validate(&format!("{path} player_fall"))?;
        if let Some(random_items) = &entry.random_items {
            random_items.validate(&format!("{path} random_items"))?;
        }
        entry.power_ups.validate(&format!("{path} power_ups"))?;
        entry.placed_items.validate(&format!("{path} placed_items"))?;
        validate_quests(&entry.quests, actors, &format!("{path} quests"))?;
    }
    Ok(())
}

pub(super) fn validate_map_registry<'a>(names: impl IntoIterator<Item = &'a str>, default_map: &str) -> Result<()> {
    let mut seen = HashSet::new();
    for name in names {
        if name.is_empty() {
            bail!("map name must not be empty");
        }
        if !is_valid_map_name(name) {
            bail!("map name `{name}` must contain only ASCII letters, digits, `_`, or `-`");
        }
        if !seen.insert(name) {
            bail!("maps contains duplicate map name {name:?}");
        }
    }
    if seen.is_empty() {
        bail!("maps must define at least one map");
    }
    if !seen.contains(default_map) {
        let mut known: Vec<&str> = seen.into_iter().collect();
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
