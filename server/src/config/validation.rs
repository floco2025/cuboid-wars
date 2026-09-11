use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use serde::{Deserialize, Deserializer};

use super::{Quest, QuestKind, RandomItemsConfig, ServerGameplayConfig};
use crate::map::MapConfig;
use common::protocol::{ItemType, SwitchId};

// A per-actor-kind map must name every configured kind (a missing entry
// silently defaulting is the footgun) and nothing else (a typo).
pub(super) fn validate_covers_actor_kinds<'a, T>(
    keys: impl Iterator<Item = &'a String>,
    actors: &HashMap<String, T>,
    path: &str,
) -> Result<()> {
    let keys: HashSet<&String> = keys.collect();
    for kind in actors.keys() {
        if !keys.contains(kind) {
            bail!("{path} is missing actor kind {kind:?}");
        }
    }
    for kind in keys {
        if !actors.contains_key(kind) {
            bail!("{path} contains unknown actor kind {kind:?}");
        }
    }
    Ok(())
}

pub(crate) fn validate_map_actor_kinds(config: &ServerGameplayConfig, map_config: &MapConfig) -> Result<()> {
    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        if !config.actors.kinds.contains_key(&zone.kind) {
            let mut known: Vec<&str> = config.actors.kinds.keys().map(String::as_str).collect();
            known.sort_unstable();
            bail!(
                "map actor spawn zone {zone_idx} references unknown actor kind {:?} (known kinds: {known:?})",
                zone.kind
            );
        }
        if zone.switch.is_some() && config.expect_actor(&zone.kind).respawn_secs.is_none() {
            bail!(
                "map actor spawn zone {zone_idx} is operated by a switch but its kind {:?} has respawn_secs null, so it would never spawn; give the kind a respawn time",
                zone.kind
            );
        }
        if config.expect_actor(&zone.kind).character.immovable {
            let capacity = zone.immovable_cells(map_config.grid(zone.carrier)).count();
            if zone.count as usize > capacity {
                bail!(
                    "map actor spawn zone {zone_idx} on carrier {} requests {} immovable {:?} actors but has only {capacity} usable floor cells",
                    zone.carrier.0,
                    zone.count,
                    zone.kind
                );
            }
        }
    }
    Ok(())
}

// The fireworks switch resolved, and operated by some plate of the map.
pub(crate) fn validate_map_quests(
    quests: &[Quest],
    map_config: &MapConfig,
    random_items: Option<&RandomItemsConfig>,
    fireworks_switch: Option<SwitchId>,
) -> Result<()> {
    for quest in quests {
        let available = match quest.kind {
            QuestKind::ActorKills => map_config
                .actor_spawn_zones
                .iter()
                .any(|zone| quest.actor_kind.as_ref().is_none_or(|kind| zone.kind == *kind)),
            QuestKind::Gold => {
                map_config
                    .placed_items
                    .iter()
                    .any(|item| item.item_type == ItemType::Gold)
                    || random_items.is_some_and(|items| items.types.iter().any(|item| item == "gold"))
            }
            QuestKind::Fireworks => fireworks_switch.is_some(),
        };
        if !available {
            bail!(
                "quest {:?} cannot be completed on the selected map: its required world content is absent",
                quest.id.0
            );
        }
    }
    Ok(())
}

pub(super) use common::config::{validate_non_negative_finite, validate_positive_finite};

// Serde fills an absent `Option` field with `None`; routing it through
// `deserialize_with` makes the key mandatory, so `null` is always a choice.
pub(super) fn deserialize_required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[cfg(test)]
#[path = "tests/validation.rs"]
mod tests;
