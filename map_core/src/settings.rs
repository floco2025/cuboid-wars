//! The effective settings of a map: the `gameplay.json` defaults with the
//! map's `settings.json` overrides applied, computed the same way by the
//! server and the editor.
use anyhow::{Result, bail};
use serde_json::{Map, Value};

// Sections every map file supplies itself; the defaults never carry them.
pub const MAP_CONTENT_KEYS: [&str; 5] = ["textures", "grounds", "random_items", "placed_items", "quests"];
// Keys of gameplay.json that describe the server rather than a map.
pub const GLOBAL_KEYS: [&str; 3] = ["network", "default_map", "maps"];
// An object carrying one of these keys is one variant of a tagged union.
const VARIANT_TAGS: [&str; 2] = ["mode", "type"];

// Applies a map's overrides to the defaults. Objects merge key by key, and an
// override key must exist in the defaults, so a typo or an added actor kind is
// an error rather than a silent no-op; scalars, arrays, and null replace. A
// tagged object replaces the default whole when its tag differs, since the
// other variant's keys are legitimately unknown to it, and merges like any
// object when the tag matches. Content keys come from the map alone.
pub fn merge_map_settings(defaults: &Value, map: &Value) -> Result<Value> {
    let Some(defaults) = defaults.as_object() else {
        bail!("the gameplay defaults must be an object");
    };
    let Some(map) = map.as_object() else {
        bail!("the map settings must be an object");
    };
    for key in MAP_CONTENT_KEYS {
        if defaults.contains_key(key) {
            bail!("{key} is map content and has no default; remove it from gameplay.json");
        }
    }
    let mut merged = Map::new();
    for (key, default) in defaults {
        let value = match map.get(key) {
            Some(value) => merge_value(default, value, key)?,
            None => default.clone(),
        };
        merged.insert(key.clone(), value);
    }
    for (key, value) in map {
        if GLOBAL_KEYS.contains(&key.as_str()) {
            bail!("{key} is global; set it in gameplay.json");
        }
        if MAP_CONTENT_KEYS.contains(&key.as_str()) {
            merged.insert(key.clone(), value.clone());
        } else if !defaults.contains_key(key) {
            bail!("{key} is not a key in the defaults; a map may only override keys that exist");
        }
    }
    Ok(Value::Object(merged))
}

fn merge_value(default: &Value, value: &Value, path: &str) -> Result<Value> {
    let (Some(default), Some(overrides)) = (default.as_object(), value.as_object()) else {
        return Ok(value.clone());
    };
    if let Some(tag) = VARIANT_TAGS.iter().find(|tag| default.contains_key(**tag))
        && overrides.get(*tag).is_some_and(|variant| variant != &default[*tag])
    {
        return Ok(value.clone());
    }
    let mut merged = Map::new();
    for (key, sub_default) in default {
        let sub = match overrides.get(key) {
            Some(sub) => merge_value(sub_default, sub, &format!("{path}.{key}"))?,
            None => sub_default.clone(),
        };
        merged.insert(key.clone(), sub);
    }
    for key in overrides.keys() {
        if !default.contains_key(key) {
            bail!("{path}.{key} is not a key in the defaults; a map may only override keys that exist");
        }
    }
    Ok(Value::Object(merged))
}

#[cfg(test)]
#[path = "tests/settings.rs"]
mod tests;
