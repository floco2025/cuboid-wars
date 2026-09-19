use std::collections::{BTreeMap, HashSet};

use anyhow::{Result, bail};
use map_core::is_valid_map_name;
use serde::Deserialize;

use super::validation::validate_positive_finite;
use common::protocol::ItemType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherMode {
    Clear,
    Rain,
    Auto,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RandomItemsConfig {
    // `ItemType` config ids. Keys are rejected — they're parameterized by
    // barrier kind and must be placed in the map's `items` list.
    pub weights: BTreeMap<String, f64>,
    pub max_number: usize,
    // How long an uncollected random item sits in the world before being
    // removed. Placed items use the map's `placed_items.respawn_secs` instead.
    pub despawn_secs: f32,
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
    pub(super) fn validate(&self, path: &str) -> Result<()> {
        if self.weights.is_empty() {
            bail!("{path}.weights must not be empty");
        }
        let mut total_weight = 0.0;
        for (ty, &weight) in &self.weights {
            if ty == ItemType::KEY_CONFIG_ID {
                bail!(
                    "{path}.weights: keys are parameterized by barrier kind and cannot spawn randomly; place them in the map's `items` list"
                );
            }
            if ItemType::from_config_id(ty).is_none() {
                bail!("{path}.weights contains unknown item type {ty:?}");
            }
            if !weight.is_finite() || weight < 0.0 {
                bail!("{path}.weights.{ty} must be finite and >= 0");
            }
            total_weight += weight;
        }
        if total_weight == 0.0 {
            bail!("{path}.weights must contain at least one positive weight");
        }
        if !total_weight.is_finite() {
            bail!("{path}.weights total must be finite");
        }
        if self.max_number == 0 {
            bail!("{path}.max_number must be >= 1");
        }
        validate_positive_finite(self.despawn_secs, &format!("{path}.despawn_secs"))
    }
}
