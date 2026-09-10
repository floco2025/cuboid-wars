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
            QuestKind::Gold => {
                map_config
                    .placed_items
                    .iter()
                    .any(|item| item.item_type == ItemType::Gold)
                    || random_items.is_some_and(|items| items.types.iter().any(|item| item == "gold"))
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
    use crate::{
        map::{ActorSpawnZone, CarrierGrid, CellGrid, EdgeGrid, LevelGrid},
        test_geometry::geometry,
    };
    use common::protocol::CarrierId;

    fn config_and_map() -> (ServerGameplayConfig, MapConfig) {
        let server = ServerGameplayConfig::load_default().expect("load server gameplay");
        let settings = &server.maps.get("hotel").expect("hotel settings missing").settings;
        let (barrier_kinds, bridge_kinds) = settings.kind_tables().expect("hotel kind tables rejected");
        let map = crate::map::generate_map("hotel", 30, settings, &barrier_kinds, &bridge_kinds)
            .expect("hotel map failed to generate")
            .config;
        (server, map)
    }

    #[test]
    fn immovable_capacity_counts_only_usable_floors_on_the_zones_carrier() {
        let server = ServerGameplayConfig::load_default().expect("gameplay config rejected");
        let mut map = MapConfig::for_grid(Vec::new(), geometry(4, 1));
        let mut cells = CellGrid::new(4, 1);
        cells.rows[0][0].has_floor = true;
        cells.rows[0][1].has_floor = true;
        cells.rows[0][1].has_ramp = true;
        cells.rows[0][2].has_floor_slab = true;
        map.grids.push(CarrierGrid::new(
            CarrierId(1),
            geometry(4, 1),
            vec![LevelGrid {
                cells,
                edges: EdgeGrid::new(4, 1),
                barrier_edges: EdgeGrid::new(4, 1),
            }],
        ));
        map.actor_spawn_zones.push(ActorSpawnZone {
            carrier: CarrierId(1),
            level: 0,
            cols: [0, 4],
            rows: [0, 1],
            kind: "turret".into(),
            count: 1,
        });
        validate_map_actor_kinds(&server, &map).expect("one turret rejected");
        map.actor_spawn_zones[0].count = 2;
        let error = validate_map_actor_kinds(&server, &map).expect_err("overfilled immovable zone accepted");
        assert!(error.to_string().contains("only 1 usable floor cells"), "{error}");
        assert!(error.to_string().contains("carrier 1"), "{error}");
        map.actor_spawn_zones[0].kind = "scuttler".into();
        map.actor_spawn_zones[0].count = 100;
        validate_map_actor_kinds(&server, &map).expect("movable actor count limited by cell count");
    }

    #[test]
    fn missing_server_actor_kind_is_rejected() {
        let (mut server, map) = config_and_map();
        server.actors.kinds.remove("scuttler");

        let error = validate_map_actor_kinds(&server, &map).expect_err("missing server actor kind must fail");

        assert!(error.to_string().contains("unknown actor kind"));
    }
}
