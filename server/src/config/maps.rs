use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::validation::validate_positive_finite;
use common::protocol::{ItemType, MapSettings};

// Server-side wrapper around the wire `MapSettings`: the flattened settings
// ship to clients in `SInit`, while the rest stays server-only.
#[derive(Debug, Clone, Deserialize)]
pub struct MapServerConfig {
    #[serde(flatten)]
    pub settings: MapSettings,
    // `None` = no random item spawning on this map.
    #[serde(default)]
    pub random_items: Option<RandomItemsConfig>,
    // A concrete state holds until an admin command; `auto` runs the
    // global `weather_cycle`. Mirrors `/weather rain|clear|auto`.
    #[serde(default)]
    pub weather: WeatherMode,
    // A concrete look holds until an admin command; `auto` runs the
    // global `lighting_cycle`. Mirrors `/light bright|dim|dark|auto`.
    #[serde(default)]
    pub lighting: LightingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherMode {
    #[default]
    Clear,
    Rain,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightingMode {
    #[default]
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
    // removed. Placed items use `placed_items.respawn_secs` instead.
    pub despawn_secs: f32,
}

pub(super) fn validate_maps(maps: &HashMap<String, MapServerConfig>, default_map: &str) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_map_entry() -> MapServerConfig {
        MapServerConfig {
            settings: MapSettings {
                skybox: "cloudy_day".to_owned(),
                gravity: 25.0,
                low_gravity: 5.0,
            },
            random_items: None,
            weather: WeatherMode::Clear,
            lighting: LightingMode::Bright,
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
    fn map_entry_defaults_to_clear_and_bright() {
        let entry: MapServerConfig =
            serde_json::from_str(r#"{"skybox": "cloudy_day", "gravity": 25.0, "low_gravity": 5.0}"#)
                .expect("minimal map entry should deserialize");
        assert_eq!(entry.weather, WeatherMode::Clear);
        assert_eq!(entry.lighting, LightingMode::Bright);
    }

    #[test]
    fn map_entry_parses_snake_case_weather_and_lighting() {
        let entry: MapServerConfig = serde_json::from_str(
            r#"{"skybox": "cloudy_day", "gravity": 25.0, "low_gravity": 5.0, "weather": "rain", "lighting": "dark"}"#,
        )
        .expect("map entry with weather and lighting should deserialize");
        assert_eq!(entry.weather, WeatherMode::Rain);
        assert_eq!(entry.lighting, LightingMode::Dark);
    }

    #[test]
    fn map_entry_parses_auto_modes() {
        let entry: MapServerConfig = serde_json::from_str(
            r#"{"skybox": "cloudy_day", "gravity": 25.0, "low_gravity": 5.0, "weather": "auto", "lighting": "auto"}"#,
        )
        .expect("map entry with auto modes should deserialize");
        assert_eq!(entry.weather, WeatherMode::Auto);
        assert_eq!(entry.lighting, LightingMode::Auto);
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
}
