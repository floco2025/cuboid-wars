use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::validation::{deserialize_required_option, validate_covers_actor_kinds, validate_positive_finite};
use super::{
    items::PlacedItemsConfig,
    quests::{Quest, validate_quests},
};
use common::protocol::{BarrierKindTable, BridgeKindTable, ItemType, MapSettings, MapWeaponSettings};

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

pub(super) fn validate_maps<T>(
    maps: &HashMap<String, MapServerConfig>,
    default_map: &str,
    actors: &HashMap<String, T>,
) -> Result<()> {
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
        BarrierKindTable::from_defs(entry.settings.barrier_kind_defs())
            .with_context(|| format!("invalid {path}.barrier_kinds"))?;
        BridgeKindTable::from_defs(entry.settings.bridge_kind_defs())
            .with_context(|| format!("invalid {path}.bridge_kinds"))?;
        let movement_path = format!("{path}.movement");
        let movement = &entry.settings.movement;
        validate_covers_actor_kinds(movement.actors.keys(), actors, &format!("{movement_path}.actors"))?;
        movement.validate(&movement_path)?;
        if let Some(random_items) = &entry.random_items {
            random_items.validate(&format!("{path}.random_items"), entry.settings.weapons)?;
        }
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
    fn validate(&self, path: &str, weapons: MapWeaponSettings) -> Result<()> {
        if self.types.is_empty() {
            bail!("{path}.types must not be empty");
        }
        let mut seen: HashSet<&str> = HashSet::with_capacity(self.types.len());
        let mut spawnable = 0usize;
        for ty in &self.types {
            if ty == ItemType::KEY_CONFIG_ID {
                bail!(
                    "{path}.types: keys are parameterized by barrier kind and cannot spawn randomly; place them in the map's `items` list"
                );
            }
            let Some(item_type) = ItemType::from_config_id(ty) else {
                bail!("{path}.types contains unknown item type {ty:?}");
            };
            if !seen.insert(ty.as_str()) {
                bail!("{path}.types contains duplicate {ty:?}");
            }
            spawnable += usize::from(weapons.allows_item(item_type));
        }
        if spawnable == 0 {
            bail!("{path}.types holds only pickups for weapons this map disables");
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
    use common::protocol::{HexColor, KindDef};
    use common::{
        config::{ActorMovementConfig, KnockbackConfig, MapMovementConfig, PlayerMovementConfig},
        protocol::{MapWeaponSettings, PortalMode},
    };

    fn actor_kinds() -> HashMap<String, ()> {
        ["mine", "sentry", "reaper", "zapper"]
            .into_iter()
            .map(|kind| (kind.to_owned(), ()))
            .collect()
    }

    fn ok_movement() -> MapMovementConfig {
        MapMovementConfig {
            player: PlayerMovementConfig {
                walk_speed: 6.0,
                run_speed: 9.0,
                speed_power_up: 1.6,
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
                movement: ok_movement(),
                weapons: MapWeaponSettings {
                    projectiles: true,
                    missiles: true,
                    portals: PortalMode::Both,
                },
                barrier_kinds: None,
                bridge_kinds: None,
            },
            random_items: None,
            placed_items: ok_placed_items(),
            weather: WeatherMode::Clear,
            lighting: LightingMode::Bright,
            quests: Vec::new(),
        }
    }

    fn ok_placed_items() -> PlacedItemsConfig {
        PlacedItemsConfig {
            respawn_secs: crate::config::PlacedItemRespawnSecs {
                speed: 60.0,
                multi_shot: 60.0,
                low_gravity: 60.0,
                health_potion: 60.0,
                cookie: 60.0,
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
        projectiles: bool,
        missiles: bool,
        portals: &str,
        weather: Option<&str>,
        lighting: Option<&str>,
    ) -> Result<MapServerConfig, serde_json::Error> {
        let mut value = serde_json::json!({
            "skybox": "cloudy_day",
            "movement": {
                "player": { "walk_speed": 6.0, "run_speed": 9.0, "speed_power_up": 1.6 },
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
            "weapons": { "projectiles": projectiles, "missiles": missiles, "portals": portals },
            "barrier_kinds": null,
            "bridge_kinds": null,
            "random_items": null,
            "placed_items": {
                "respawn_secs": {
                    "speed": 60.0,
                    "multi_shot": 60.0,
                    "low_gravity": 60.0,
                    "health_potion": 60.0,
                    "cookie": 60.0,
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
        let missing_both =
            parse_map_entry(true, true, "both", None, None).expect_err("weather and lighting must be explicit");
        assert!(missing_both.to_string().contains("weather"));

        let missing_lighting =
            parse_map_entry(true, true, "both", Some("clear"), None).expect_err("lighting must be explicit");
        assert!(missing_lighting.to_string().contains("lighting"));
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
            .cookie = -1.0;

        let error = validate_test_maps(&maps, "hotel").expect_err("negative respawn time must be rejected");
        assert!(
            error
                .to_string()
                .contains("maps.hotel.placed_items.respawn_secs.cookie")
        );
    }

    #[test]
    fn map_entry_accepts_null_barrier_kinds() {
        let entry =
            parse_map_entry(true, true, "both", Some("clear"), Some("bright")).expect("map entry should deserialize");
        assert!(entry.settings.barrier_kinds.is_none());
    }

    #[test]
    fn validate_maps_rejects_duplicate_barrier_kinds() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel")
            .expect("hotel entry missing")
            .settings
            .barrier_kinds = Some(vec![kind("lobby"), kind("lobby")]);

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
            .bridge_kinds = Some(vec![kind("skyway"), kind("skyway")]);

        let error = validate_test_maps(&maps, "hotel").expect_err("duplicate bridge kinds must be rejected");
        assert!(error.to_string().contains("maps.hotel.bridge_kinds"));
    }

    #[test]
    fn map_entry_parses_snake_case_weather_and_lighting() {
        let entry =
            parse_map_entry(true, true, "both", Some("rain"), Some("dark")).expect("map entry should deserialize");
        assert_eq!(entry.weather, WeatherMode::Rain);
        assert_eq!(entry.lighting, LightingMode::Dark);
    }

    #[test]
    fn map_entry_parses_auto_modes() {
        let entry =
            parse_map_entry(true, true, "both", Some("auto"), Some("auto")).expect("map entry should deserialize");
        assert_eq!(entry.weather, WeatherMode::Auto);
        assert_eq!(entry.lighting, LightingMode::Auto);
    }

    #[test]
    fn map_entry_accepts_all_disabled_weapons_and_single_portals() {
        let disabled =
            parse_map_entry(false, false, "none", Some("clear"), Some("bright")).expect("map entry should deserialize");
        assert!(!disabled.settings.weapons.projectiles);
        assert!(!disabled.settings.weapons.missiles);
        assert_eq!(disabled.settings.weapons.portals, PortalMode::None);

        let single =
            parse_map_entry(true, true, "single", Some("clear"), Some("bright")).expect("map entry should deserialize");
        assert_eq!(single.settings.weapons.portals, PortalMode::Single);
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
        let maps = one_map_with_random_items("hotel", ok_random_items(&["speed", "cookie"]));
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
    fn validate_maps_rejects_random_pool_of_only_disabled_weapon_pickups() {
        let mut maps = one_map_with_random_items("hotel", ok_random_items(&["missile_pack", "multi_shot"]));
        let entry = maps.get_mut("hotel").expect("hotel entry missing");
        entry.settings.weapons.missiles = false;
        entry.settings.weapons.projectiles = false;
        let err = validate_test_maps(&maps, "hotel").expect_err("fully disabled pool must be rejected");
        assert!(err.to_string().contains("disables"));

        maps.get_mut("hotel")
            .expect("hotel entry missing")
            .random_items
            .as_mut()
            .expect("random_items missing")
            .types
            .push("cookie".to_owned());
        validate_test_maps(&maps, "hotel").expect("a pool with one spawnable pickup should pass");
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
