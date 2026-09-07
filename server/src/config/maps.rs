use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::{
    actors::ActorKindServerConfig,
    items::{PlacedItemsConfig, PowerUpsConfig},
    quests::{Quest, validate_quests},
    validation::{deserialize_required_option, validate_covers_actor_kinds, validate_positive_finite},
};
use common::protocol::{BarrierKindTable, BridgeKindTable, ItemType, MapSettings, validate_texture_catalog};

// Server-side wrapper around the wire `MapSettings`: the flattened settings
// ship to clients in `SInit`, while the rest stays server-only.
#[derive(Debug, Clone, Deserialize)]
pub struct MapServerConfig {
    #[serde(flatten)]
    pub settings: MapSettings,
    // `None` = no random item spawning on this map.
    #[serde(deserialize_with = "deserialize_required_option")]
    pub random_items: Option<RandomItemsConfig>,
    pub placed_items: PlacedItemsConfig,
    pub power_ups: PowerUpsConfig,
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
) -> Result<()> {
    if maps.is_empty() {
        bail!("maps must define at least one map");
    }
    let movable_actors: HashMap<_, _> = actors
        .iter()
        .filter(|(_, actor)| !actor.character.immovable)
        .map(|(kind, actor)| (kind.clone(), actor))
        .collect();
    for (name, entry) in maps {
        // Map names become file names (`config/server/maps/<name>.json`), so
        // reject anything that could traverse paths.
        if name.is_empty() {
            bail!("map name must not be empty");
        }
        if !is_valid_map_name(name) {
            bail!("map name `{name}` must contain only ASCII letters, digits, `_`, or `-`");
        }
        let path = format!("maps.{name}");
        if entry.settings.skybox.is_empty() {
            bail!("{path}.skybox must not be empty");
        }
        BarrierKindTable::from_defs(&entry.settings.barrier_kinds)
            .with_context(|| format!("invalid {path}.barrier_kinds"))?;
        BridgeKindTable::from_defs(&entry.settings.bridge_kinds)
            .with_context(|| format!("invalid {path}.bridge_kinds"))?;
        validate_texture_catalog(&entry.settings.textures, &format!("{path}.textures"))?;
        entry.settings.geometry.validate(&format!("{path}.geometry"))?;
        let movement_path = format!("{path}.movement");
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
        if let Some(random_items) = &entry.random_items {
            random_items.validate(&format!("{path}.random_items"))?;
        }
        entry.power_ups.validate(&format!("{path}.power_ups"))?;
        entry.placed_items.validate(&format!("{path}.placed_items"))?;
        validate_quests(&entry.quests, actors, &format!("{path}.quests"))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_geometry::sizes;
    use common::{
        config::{ActorMovementConfig, KnockbackConfig, MapMovementConfig, PlayerMovementConfig},
        protocol::{HexColor, KindDef, PortalMode},
    };

    fn actor_kinds() -> HashMap<String, ActorKindServerConfig> {
        let mut actors = crate::config::ServerGameplayConfig::load_default()
            .expect("gameplay config rejected")
            .actors
            .kinds;
        actors.remove("turret");
        actors
    }

    fn ok_movement() -> MapMovementConfig {
        MapMovementConfig {
            player: PlayerMovementConfig {
                walk_speed: 6.0,
                run_speed: 9.0,
                speed_power_up: 1.6,
                jump_speed: 12.0,
            },
            actors: [
                ("mine", 3.0, 5.0),
                ("sentry", 5.0, 8.0),
                ("reaper", 5.0, 8.0),
                ("zapper", 2.0, 4.0),
            ]
            .into_iter()
            .map(|(kind, roam_speed, active_speed)| {
                (
                    kind.to_owned(),
                    ActorMovementConfig {
                        roam_speed,
                        active_speed,
                    },
                )
            })
            .collect(),
            missile_speed: 16.0,
            projectile_speed: 90.0,
            gravity: 25.0,
            low_gravity: 5.0,
            ladder_climb_ratio: 0.4,
            knockback: KnockbackConfig {
                max_speed: 15.0,
                up_speed: 7.0,
                deceleration: 35.0,
            },
        }
    }

    fn kind(id: &str) -> KindDef {
        KindDef {
            id: id.to_owned(),
            color: HexColor([0; 3]),
        }
    }

    fn ok_map_entry() -> MapServerConfig {
        MapServerConfig {
            settings: MapSettings {
                skybox: "cloudy_day".to_owned(),
                textures: Default::default(),

                geometry: sizes(),
                movement: ok_movement(),
                portals: PortalMode::Both,
                barrier_kinds: Vec::new(),
                bridge_kinds: Vec::new(),
            },
            random_items: None,
            placed_items: ok_placed_items(),
            power_ups: PowerUpsConfig {
                duration_secs: crate::config::PowerUpDurationSecs {
                    speed: 30.0,
                    single_shot: 0.0,
                    multi_shot: 25.0,
                    low_gravity: 20.0,
                    portal_gun: 0.0,
                },
            },
            weather: WeatherMode::Clear,
            lighting: LightingMode::Bright,
            quests: Vec::new(),
        }
    }

    fn ok_placed_items() -> PlacedItemsConfig {
        PlacedItemsConfig {
            respawn_secs: crate::config::PlacedItemRespawnSecs {
                speed: 60.0,
                single_shot: 0.0,
                multi_shot: 60.0,
                low_gravity: 60.0,
                portal_gun: 0.0,
                health_potion: 60.0,
                gold: 60.0,
                key: 30.0,
                missile_pack: 30.0,
            },
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

    fn validate_test_maps(maps: &HashMap<String, MapServerConfig>, default_map: &str) -> Result<()> {
        validate_maps(maps, default_map, &actor_kinds())
    }

    fn parse_map_entry(
        portals: &str,
        weather: Option<&str>,
        lighting: Option<&str>,
    ) -> Result<MapServerConfig, serde_json::Error> {
        let mut value = serde_json::json!({
            "skybox": "cloudy_day",
            "textures": {},
            "geometry": { "grid_cell_size": 3.4, "level_height": 4.4, "floor_thickness": 0.4, "wall_thickness": 0.3 },
            "movement": {
                "player": { "walk_speed": 6.0, "run_speed": 9.0, "speed_power_up": 1.6, "jump_speed": 12.0 },
                "actors": {
                    "mine": { "roam_speed": 3.0, "active_speed": 5.0 },
                    "sentry": { "roam_speed": 5.0, "active_speed": 8.0 },
                    "reaper": { "roam_speed": 5.0, "active_speed": 8.0 },
                    "zapper": { "roam_speed": 2.0, "active_speed": 4.0 }
                },
                "missile_speed": 16.0,
                "projectile_speed": 90.0,
                "gravity": 25.0,
                "low_gravity": 5.0,
                "ladder_climb_ratio": 0.4,
                "knockback": { "max_speed": 15.0, "up_speed": 7.0, "deceleration": 35.0 }
            },
            "portals": portals,
            "barrier_kinds": [],
            "bridge_kinds": [],
            "random_items": null,
            "power_ups": { "duration_secs": { "speed": 30.0, "single_shot": 0.0, "multi_shot": 25.0, "low_gravity": 20.0, "portal_gun": 0.0 } },
            "placed_items": {
                "respawn_secs": {
                    "speed": 60.0,
                    "single_shot": 5.0,
                    "multi_shot": 60.0,
                    "low_gravity": 60.0,
                    "portal_gun": 1.0,
                    "health_potion": 60.0,
                    "gold": 60.0,
                    "key": 30.0,
                    "missile_pack": 30.0
                }
            },
            "quests": []
        });
        let object = value.as_object_mut().expect("map entry JSON is not an object");
        if let Some(weather) = weather {
            object.insert("weather".to_owned(), weather.into());
        }
        if let Some(lighting) = lighting {
            object.insert("lighting".to_owned(), lighting.into());
        }
        serde_json::from_value(value)
    }

    #[test]
    fn validate_maps_accepts_single_valid_entry() {
        validate_test_maps(&one_map("hotel"), "hotel").expect("valid map registry should pass");
    }

    #[test]
    fn validate_maps_rejects_empty_registry() {
        let err = validate_test_maps(&HashMap::new(), "hotel").expect_err("empty registry must be rejected");
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn validate_maps_rejects_unknown_default_map() {
        let err = validate_test_maps(&one_map("hotel"), "lobby").expect_err("unknown default must be rejected");
        assert!(err.to_string().contains("default_map"));
    }

    #[test]
    fn validate_maps_rejects_path_unsafe_name() {
        let err = validate_test_maps(&one_map("../hotel"), "../hotel").expect_err("path chars must be rejected");
        assert!(err.to_string().contains("ASCII"));
    }

    #[test]
    fn validate_maps_rejects_non_positive_gravity() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel")
            .expect("hotel entry missing")
            .settings
            .movement
            .gravity = 0.0;
        let err = validate_test_maps(&maps, "hotel").expect_err("zero gravity must be rejected");
        assert!(err.to_string().contains("gravity"));
    }

    #[test]
    fn validate_maps_rejects_non_positive_cell_size() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel")
            .expect("hotel entry missing")
            .settings
            .geometry
            .grid_cell_size = 0.0;
        let err = validate_test_maps(&maps, "hotel").expect_err("zero cell size must be rejected");
        assert!(err.to_string().contains("maps.hotel.geometry.grid_cell_size"));
    }

    #[test]
    fn validate_maps_rejects_non_positive_player_speed() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel")
            .expect("hotel entry missing")
            .settings
            .movement
            .player
            .run_speed = 0.0;
        let err = validate_test_maps(&maps, "hotel").expect_err("zero run speed must be rejected");
        assert!(err.to_string().contains("movement.player.run_speed"));
    }

    #[test]
    fn validate_maps_rejects_missing_actor_movement() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel")
            .expect("hotel entry missing")
            .settings
            .movement
            .actors
            .remove("mine");
        let err = validate_test_maps(&maps, "hotel").expect_err("missing actor movement must be rejected");
        assert!(err.to_string().contains("movement.actors"));
        assert!(err.to_string().contains("mine"));
    }

    #[test]
    fn validate_maps_rejects_unknown_actor_movement() {
        let mut maps = one_map("hotel");
        let movement = maps
            .get_mut("hotel")
            .expect("hotel entry missing")
            .settings
            .movement
            .actors
            .get("zapper")
            .copied()
            .expect("zapper movement missing");
        maps.get_mut("hotel")
            .expect("hotel entry missing")
            .settings
            .movement
            .actors
            .insert("banana".to_owned(), movement);
        let err = validate_test_maps(&maps, "hotel").expect_err("unknown actor movement must be rejected");
        assert!(err.to_string().contains("movement.actors"));
        assert!(err.to_string().contains("banana"));
    }

    #[test]
    fn validate_maps_rejects_negative_low_gravity() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel")
            .expect("hotel entry missing")
            .settings
            .movement
            .low_gravity = -1.0;
        let err = validate_test_maps(&maps, "hotel").expect_err("negative low_gravity must be rejected");
        assert!(err.to_string().contains("low_gravity"));
    }

    #[test]
    fn validate_maps_rejects_empty_skybox() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel").expect("hotel entry missing").settings.skybox = String::new();
        let err = validate_test_maps(&maps, "hotel").expect_err("empty skybox must be rejected");
        assert!(err.to_string().contains("skybox"));
    }

    #[test]
    fn map_entry_requires_explicit_weather_and_lighting() {
        let missing_both = parse_map_entry("both", None, None).expect_err("weather and lighting must be explicit");
        assert!(missing_both.to_string().contains("weather"));

        let missing_lighting = parse_map_entry("both", Some("clear"), None).expect_err("lighting must be explicit");
        assert!(missing_lighting.to_string().contains("lighting"));
    }

    #[test]
    fn textures_require_an_explicit_catalog_and_boolean_permissions() {
        let gameplay: serde_json::Value = serde_json::from_str(include_str!("../../../config/server/gameplay.json"))
            .expect("gameplay JSON is invalid");
        let mut hotel = gameplay["maps"]["hotel"].clone();
        hotel
            .as_object_mut()
            .expect("hotel settings is not an object")
            .remove("textures");
        assert!(
            serde_json::from_value::<MapServerConfig>(hotel)
                .expect_err("missing textures was accepted")
                .to_string()
                .contains("textures")
        );
        for permission in [serde_json::json!({}), serde_json::json!({"portalable": "false"})] {
            let mut hotel = gameplay["maps"]["hotel"].clone();
            hotel["textures"] = serde_json::json!({"stone": permission});
            assert!(serde_json::from_value::<MapServerConfig>(hotel).is_err());
        }
    }

    #[test]
    fn map_entry_requires_explicit_barrier_kinds() {
        let gameplay: serde_json::Value = serde_json::from_str(include_str!("../../../config/server/gameplay.json"))
            .expect("server gameplay JSON is invalid");
        let mut hotel = gameplay["maps"]["hotel"].clone();
        hotel
            .as_object_mut()
            .expect("hotel map settings are not an object")
            .remove("barrier_kinds");

        let error = serde_json::from_value::<MapServerConfig>(hotel)
            .expect_err("barrier_kinds must be explicit even when absent by design");
        assert!(error.to_string().contains("barrier_kinds"));
    }

    #[test]
    fn map_entry_requires_placed_items() {
        let gameplay: serde_json::Value = serde_json::from_str(include_str!("../../../config/server/gameplay.json"))
            .expect("server gameplay JSON is invalid");
        let mut hotel = gameplay["maps"]["hotel"].clone();
        hotel
            .as_object_mut()
            .expect("hotel map settings are not an object")
            .remove("placed_items");

        let error =
            serde_json::from_value::<MapServerConfig>(hotel).expect_err("placed_items must be defined for every map");
        assert!(error.to_string().contains("placed_items"));
    }

    #[test]
    fn validate_maps_rejects_negative_placed_item_respawn() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel")
            .expect("hotel entry missing")
            .placed_items
            .respawn_secs
            .gold = -1.0;

        let error = validate_test_maps(&maps, "hotel").expect_err("negative respawn time must be rejected");
        assert!(error.to_string().contains("maps.hotel.placed_items.respawn_secs.gold"));
    }

    #[test]
    fn map_entry_accepts_empty_kind_catalogs() {
        let entry = parse_map_entry("both", Some("clear"), Some("bright")).expect("map entry failed to deserialize");
        assert!(entry.settings.barrier_kinds.is_empty());
        assert!(entry.settings.bridge_kinds.is_empty());
    }

    #[test]
    fn map_entry_rejects_null_kind_catalogs() {
        for key in ["barrier_kinds", "bridge_kinds"] {
            let gameplay: serde_json::Value =
                serde_json::from_str(include_str!("../../../config/server/gameplay.json"))
                    .expect("server gameplay JSON is invalid");
            let mut hotel = gameplay["maps"]["hotel"].clone();
            hotel[key] = serde_json::Value::Null;

            let error = serde_json::from_value::<MapServerConfig>(hotel)
                .expect_err("a null kind catalog deserialized; an empty map lists []");
            assert!(error.to_string().contains("expected a sequence"), "{key}: {error}");
        }
    }

    #[test]
    fn validate_maps_rejects_duplicate_barrier_kinds() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel")
            .expect("hotel entry missing")
            .settings
            .barrier_kinds = vec![kind("lobby"), kind("lobby")];

        let error = validate_test_maps(&maps, "hotel").expect_err("duplicate barrier kinds must be rejected");
        assert!(error.to_string().contains("maps.hotel.barrier_kinds"));
    }

    #[test]
    fn map_entry_requires_explicit_bridge_kinds() {
        let gameplay: serde_json::Value = serde_json::from_str(include_str!("../../../config/server/gameplay.json"))
            .expect("server gameplay JSON is invalid");
        let mut hotel = gameplay["maps"]["hotel"].clone();
        hotel
            .as_object_mut()
            .expect("hotel map settings are not an object")
            .remove("bridge_kinds");

        let error = serde_json::from_value::<MapServerConfig>(hotel)
            .expect_err("bridge_kinds must be explicit even when absent by design");
        assert!(error.to_string().contains("bridge_kinds"));
    }

    #[test]
    fn validate_maps_rejects_duplicate_bridge_kinds() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel")
            .expect("hotel entry missing")
            .settings
            .bridge_kinds = vec![kind("skyway"), kind("skyway")];

        let error = validate_test_maps(&maps, "hotel").expect_err("duplicate bridge kinds must be rejected");
        assert!(error.to_string().contains("maps.hotel.bridge_kinds"));
    }

    #[test]
    fn map_entry_parses_snake_case_weather_and_lighting() {
        let entry = parse_map_entry("both", Some("rain"), Some("dark")).expect("map entry should deserialize");
        assert_eq!(entry.weather, WeatherMode::Rain);
        assert_eq!(entry.lighting, LightingMode::Dark);
    }

    #[test]
    fn map_entry_parses_auto_modes() {
        let entry = parse_map_entry("both", Some("auto"), Some("auto")).expect("map entry should deserialize");
        assert_eq!(entry.weather, WeatherMode::Auto);
        assert_eq!(entry.lighting, LightingMode::Auto);
    }

    #[test]
    fn map_entry_accepts_single_or_both_portal_ownership() {
        let both = parse_map_entry("both", Some("clear"), Some("bright")).expect("map entry JSON is invalid");
        assert_eq!(both.settings.portals, PortalMode::Both);

        let single = parse_map_entry("single", Some("clear"), Some("bright")).expect("map entry JSON is invalid");
        assert_eq!(single.settings.portals, PortalMode::Single);
    }

    #[test]
    fn lighting_mode_presets_match_names() {
        assert_eq!(LightingMode::Bright.preset(), Some("bright"));
        assert_eq!(LightingMode::Dim.preset(), Some("dim"));
        assert_eq!(LightingMode::Dark.preset(), Some("dark"));
        assert_eq!(LightingMode::Auto.preset(), None);
    }

    #[test]
    fn validate_maps_accepts_map_without_random_items() {
        validate_test_maps(&one_map("hotel"), "hotel").expect("map without random_items should pass");
    }

    #[test]
    fn validate_maps_accepts_valid_random_items() {
        let maps = one_map_with_random_items("hotel", ok_random_items(&["speed", "gold"]));
        validate_test_maps(&maps, "hotel").expect("valid random_items should pass");
    }

    #[test]
    fn validate_maps_rejects_key_in_random_pool() {
        let maps = one_map_with_random_items("hotel", ok_random_items(&["speed", "key"]));
        let err = validate_test_maps(&maps, "hotel").expect_err("key in random pool must be rejected");
        assert!(err.to_string().contains("barrier kind"));
    }

    #[test]
    fn validate_maps_rejects_unknown_random_item_type() {
        let maps = one_map_with_random_items("hotel", ok_random_items(&["banana"]));
        let err = validate_test_maps(&maps, "hotel").expect_err("unknown type must be rejected");
        assert!(err.to_string().contains("unknown item type"));
    }

    #[test]
    fn validate_maps_rejects_duplicate_random_item_types() {
        let maps = one_map_with_random_items("hotel", ok_random_items(&["speed", "speed"]));
        let err = validate_test_maps(&maps, "hotel").expect_err("duplicate type must be rejected");
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn validate_maps_rejects_empty_random_item_types() {
        let maps = one_map_with_random_items("hotel", ok_random_items(&[]));
        let err = validate_test_maps(&maps, "hotel").expect_err("empty pool must be rejected");
        assert!(err.to_string().contains("types"));
    }

    #[test]
    fn validate_maps_accepts_projectile_pickups_as_the_only_random_items() {
        for item in ["single_shot", "multi_shot"] {
            let maps = one_map_with_random_items("hotel", ok_random_items(&[item]));
            validate_test_maps(&maps, "hotel").expect("projectile pickup pool is invalid");
        }
    }

    #[test]
    fn validate_maps_rejects_zero_random_item_max_number() {
        let mut random_items = ok_random_items(&["speed"]);
        random_items.max_number = 0;
        let maps = one_map_with_random_items("hotel", random_items);
        let err = validate_test_maps(&maps, "hotel").expect_err("zero max_number must be rejected");
        assert!(err.to_string().contains("max_number"));
    }
}
