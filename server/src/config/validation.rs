use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use serde::{Deserialize, Deserializer};

use super::{Quest, QuestKind, RandomItemsConfig, ServerGameplayConfig};
use crate::map::MapConfig;
use common::protocol::{ItemType, PlatePurpose};

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
    }
    Ok(())
}

pub(crate) fn validate_map_quests(
    quests: &[Quest],
    map_config: &MapConfig,
    random_items: Option<&RandomItemsConfig>,
) -> Result<()> {
    for quest in quests {
        let available = match quest.kind {
            QuestKind::ActorKills => map_config
                .actor_spawn_zones
                .iter()
                .any(|zone| quest.actor_kind.as_ref().is_none_or(|kind| zone.kind == *kind)),
            QuestKind::Cookies => {
                map_config
                    .placed_items
                    .iter()
                    .any(|item| item.item_type == ItemType::Cookie)
                    || random_items.is_some_and(|items| items.types.iter().any(|item| item == "cookie"))
            }
            QuestKind::Fireworks => map_config
                .pressure_plates
                .iter()
                .any(|plate| plate.purpose == PlatePurpose::Firework),
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
mod tests {
    use super::*;

    fn config_and_map() -> (ServerGameplayConfig, MapConfig) {
        let server = ServerGameplayConfig::load_default().expect("load server gameplay");
        let map = crate::map::generate_map("hotel").expect("generate hotel map").config;
        (server, map)
    }

    #[test]
    fn shipped_actor_configs_and_map_are_consistent() {
        let (server, map) = config_and_map();
        validate_map_actor_kinds(&server, &map).expect("actor kinds validate");
    }

    #[test]
    fn shipped_hotel_quests_have_required_map_content() {
        let (server, map) = config_and_map();
        let hotel = server.maps.get("hotel").expect("hotel settings missing");
        validate_map_quests(&hotel.quests, &map, hotel.random_items.as_ref())
            .expect("hotel quest content should validate");
    }

    #[test]
    fn missing_server_actor_kind_is_rejected() {
        let (mut server, map) = config_and_map();
        server.actors.kinds.remove("mine");

        let error = validate_map_actor_kinds(&server, &map).expect_err("missing server actor kind must fail");

        assert!(error.to_string().contains("unknown actor kind"));
    }
}
