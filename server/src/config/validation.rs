use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use common::config::GameplayConfig;

use super::{ServerGameplayConfig, actors::ActorKindServerConfig};
use crate::map::MapConfig;

// A per-actor-kind map must name every configured kind (a missing entry
// silently defaulting is the footgun) and nothing else (a typo).
pub(super) fn validate_covers_actor_kinds<'a>(
    keys: impl Iterator<Item = &'a String>,
    actors: &HashMap<String, ActorKindServerConfig>,
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

pub(crate) fn validate_actor_kinds_consistent(
    gameplay_config: &GameplayConfig,
    server_gameplay_config: &ServerGameplayConfig,
    map_config: &MapConfig,
) -> Result<()> {
    let common_kinds: HashSet<&str> = gameplay_config.actors.keys().map(String::as_str).collect();
    let server_kinds: HashSet<&str> = server_gameplay_config.actors.keys().map(String::as_str).collect();
    if common_kinds != server_kinds {
        let mut only_common: Vec<&str> = common_kinds.difference(&server_kinds).copied().collect();
        let mut only_server: Vec<&str> = server_kinds.difference(&common_kinds).copied().collect();
        only_common.sort_unstable();
        only_server.sort_unstable();
        bail!(
            "actor kinds disagree between common and server gameplay configs (only in common: {only_common:?}, only in server: {only_server:?})"
        );
    }
    for (zone_idx, zone) in map_config.actor_spawn_zones.iter().enumerate() {
        if !common_kinds.contains(zone.kind.as_str()) {
            let mut known: Vec<&str> = common_kinds.iter().copied().collect();
            known.sort_unstable();
            bail!(
                "map actor spawn zone {zone_idx} references unknown actor kind {:?} (known kinds: {known:?})",
                zone.kind
            );
        }
    }
    Ok(())
}

pub(super) fn validate_positive_finite(value: f32, path: &str) -> Result<()> {
    if value.is_finite() && value > 0.0 {
        return Ok(());
    }
    bail!("{path} must be positive and finite, got {value}");
}

pub(super) fn validate_non_negative_finite(value: f32, path: &str) -> Result<()> {
    if value.is_finite() && value >= 0.0 {
        return Ok(());
    }
    bail!("{path} must be non-negative and finite, got {value}");
}

#[cfg(test)]
mod tests {
    use common::protocol::BarrierKindTable;

    use super::*;

    fn configs_and_map() -> (GameplayConfig, ServerGameplayConfig, MapConfig) {
        let gameplay = GameplayConfig::load_default().expect("load common gameplay");
        let server = ServerGameplayConfig::load_default().expect("load server gameplay");
        let barriers = BarrierKindTable::from_ids(gameplay.barrier_kinds.clone()).expect("build barrier table");
        let (_, map, _) = crate::map::generate_map(&barriers, &server.default_map).expect("generate default map");
        (gameplay, server, map)
    }

    #[test]
    fn shipped_actor_configs_and_map_are_consistent() {
        let (gameplay, server, map) = configs_and_map();
        validate_actor_kinds_consistent(&gameplay, &server, &map).expect("actor kinds validate");
    }

    #[test]
    fn missing_server_actor_kind_is_rejected() {
        let (gameplay, mut server, map) = configs_and_map();
        server.actors.remove("mine");

        let error =
            validate_actor_kinds_consistent(&gameplay, &server, &map).expect_err("missing server actor kind must fail");

        assert!(error.to_string().contains("only in common: [\"mine\"]"));
    }
}
