use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use serde::Deserialize;

use super::validation::validate_positive_finite;
use common::protocol::{ItemType, Lighting, MapSettings};

// Server-side wrapper around the wire `MapSettings`: the flattened settings
// ship to clients in `SInit`, while the rest stays server-only.
#[derive(Debug, Clone, Deserialize)]
pub struct MapServerConfig {
    #[serde(flatten)]
    pub settings: MapSettings,
    // `None` = no random item spawning on this map.
    #[serde(default)]
    pub random_items: Option<RandomItemsConfig>,
    // `None` = it never rains on this map.
    #[serde(default)]
    pub rain: Option<RainScheduleConfig>,
    // Weather at server startup; `rain` needs a `rain` schedule. Mirrors
    // `/weather rain|clear`.
    #[serde(default)]
    pub weather: StartupWeather,
    // Lighting at server startup. Mirrors `/light bright|dim|dark`.
    #[serde(default)]
    pub lighting: Lighting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupWeather {
    #[default]
    Clear,
    Rain,
}

// Cadence of the server-scheduled rain: random clear stretch, ramp in, a
// random rain stretch at full intensity, fade out, repeat.
#[derive(Debug, Clone, Deserialize)]
pub struct RainScheduleConfig {
    // When false, the scheduler never starts or ends rain on its own —
    // only `/weather rain|clear` does. Absent = the automatic cycle.
    #[serde(default = "default_rain_auto")]
    pub auto: bool,
    pub min_clear_secs: f32,
    pub max_clear_secs: f32,
    pub min_rain_secs: f32,
    pub max_rain_secs: f32,
    pub ramp_in_secs: f32,
    pub fade_out_secs: f32,
}

const fn default_rain_auto() -> bool {
    true
}

impl RainScheduleConfig {
    fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.min_clear_secs, &format!("{path}.min_clear_secs"))?;
        validate_positive_finite(self.max_clear_secs, &format!("{path}.max_clear_secs"))?;
        if self.min_clear_secs > self.max_clear_secs {
            bail!("{path}.min_clear_secs must be <= {path}.max_clear_secs");
        }
        validate_positive_finite(self.min_rain_secs, &format!("{path}.min_rain_secs"))?;
        validate_positive_finite(self.max_rain_secs, &format!("{path}.max_rain_secs"))?;
        if self.min_rain_secs > self.max_rain_secs {
            bail!("{path}.min_rain_secs must be <= {path}.max_rain_secs");
        }
        validate_positive_finite(self.ramp_in_secs, &format!("{path}.ramp_in_secs"))?;
        validate_positive_finite(self.fade_out_secs, &format!("{path}.fade_out_secs"))
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
        if let Some(rain) = &entry.rain {
            rain.validate(&format!("{path}.rain"))?;
        }
        if entry.weather == StartupWeather::Rain && entry.rain.is_none() {
            bail!("{path}.weather is `rain` but the map has no `rain` schedule");
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
            rain: None,
            weather: StartupWeather::Clear,
            lighting: Lighting::Bright,
        }
    }

    fn ok_rain_schedule() -> RainScheduleConfig {
        RainScheduleConfig {
            auto: true,
            min_clear_secs: 10.0,
            max_clear_secs: 20.0,
            min_rain_secs: 5.0,
            max_rain_secs: 8.0,
            ramp_in_secs: 2.0,
            fade_out_secs: 4.0,
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
    fn validate_maps_rejects_startup_rain_without_schedule() {
        let mut maps = one_map("hotel");
        maps.get_mut("hotel").expect("hotel entry missing").weather = StartupWeather::Rain;
        let err = validate_maps(&maps, "hotel").expect_err("startup rain without a schedule must be rejected");
        assert!(err.to_string().contains("weather"));
    }

    #[test]
    fn validate_maps_accepts_startup_rain_with_schedule() {
        let mut maps = one_map("hotel");
        let entry = maps.get_mut("hotel").expect("hotel entry missing");
        entry.weather = StartupWeather::Rain;
        entry.rain = Some(ok_rain_schedule());
        validate_maps(&maps, "hotel").expect("startup rain with a schedule should pass");
    }

    #[test]
    fn map_entry_defaults_to_clear_and_bright() {
        let entry: MapServerConfig =
            serde_json::from_str(r#"{"skybox": "cloudy_day", "gravity": 25.0, "low_gravity": 5.0}"#)
                .expect("minimal map entry should deserialize");
        assert_eq!(entry.weather, StartupWeather::Clear);
        assert_eq!(entry.lighting, Lighting::Bright);
    }

    #[test]
    fn map_entry_parses_snake_case_weather_and_lighting() {
        let entry: MapServerConfig = serde_json::from_str(
            r#"{"skybox": "cloudy_day", "gravity": 25.0, "low_gravity": 5.0, "weather": "rain", "lighting": "dark"}"#,
        )
        .expect("map entry with weather and lighting should deserialize");
        assert_eq!(entry.weather, StartupWeather::Rain);
        assert_eq!(entry.lighting, Lighting::Dark);
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
