use std::{collections::HashMap, fmt::Debug, hash::Hash};

use anyhow::{Result, anyhow, bail};
use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::color::HexColor;
use crate::config::PressureSwitchConfig;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Deserialize)]
pub struct KindDef {
    pub id: String,
    pub color: HexColor,
    pub pressure_switch: PressureSwitchConfig,
}

// A kind id: a stable on-wire index into one of the selected map's ordered
// kind catalogs. The server ships those catalogs in `SInit` so both sides
// assign the same indices. `MAX` caps a catalog
// whose kinds each own a Rapier collision group (the bit budget is laid out
// in `physics/world/colliders.rs`); `None` when the kinds share one group.
pub trait KindId: Copy + Debug + Eq + Hash + Ord + Send + Sync + 'static {
    const MAX: Option<usize>;
    // The `settings.json` key and the singular noun, for error messages.
    const CONFIG_KEY: &'static str;
    const NOUN: &'static str;
    fn from_index(index: u16) -> Self;
    fn index(self) -> u16;
}

// Identity-only table: maps a kind id ↔ its string id. The client builds a
// parallel color resource at startup; the server doesn't need colors at all.
#[derive(Resource, Debug, Clone)]
pub struct KindTable<K: KindId> {
    ids: Vec<String>,
    index_by_id: HashMap<String, K>,
}

// Derived `Default` would demand `K: Default`, which an id has no use for.
impl<K: KindId> Default for KindTable<K> {
    fn default() -> Self {
        Self {
            ids: Vec::new(),
            index_by_id: HashMap::new(),
        }
    }
}

impl<K: KindId> KindTable<K> {
    pub fn from_defs(defs: &[KindDef]) -> Result<Self> {
        Self::from_ids(defs.iter().map(|def| def.id.clone()).collect())
    }

    pub fn from_ids(ids: Vec<String>) -> Result<Self> {
        if let Some(max) = K::MAX
            && ids.len() > max
        {
            bail!(
                "{} has {} entries; max is {max} (limited by available Rapier collision groups)",
                K::CONFIG_KEY,
                ids.len(),
            );
        }
        let mut index_by_id = HashMap::with_capacity(ids.len());
        for (idx, id) in ids.iter().enumerate() {
            if id.is_empty() {
                bail!("{}[{idx}] is empty", K::CONFIG_KEY);
            }
            let kind = K::from_index(
                u16::try_from(idx)
                    .map_err(|_| anyhow!("{} exceeds {} entries (u16 index overflow)", K::CONFIG_KEY, u16::MAX))?,
            );
            if index_by_id.insert(id.clone(), kind).is_some() {
                bail!("{} contains duplicate id {id:?}", K::CONFIG_KEY);
            }
        }
        Ok(Self { ids, index_by_id })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    #[must_use]
    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<K> {
        self.index_by_id.get(id).copied()
    }

    #[must_use]
    pub fn id(&self, kind: K) -> Option<&str> {
        self.ids.get(usize::from(kind.index())).map(String::as_str)
    }

    // Resolve a string id, returning a helpful error if it isn't registered.
    pub fn resolve(&self, id: &str) -> Result<K> {
        self.index_of(id).ok_or_else(|| {
            let known = self.ids.join(", ");
            anyhow!("unknown {} kind {id:?}; known kinds: [{known}]", K::NOUN)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{BarrierKindId, BarrierKindTable, BridgeKindId, BridgeKindTable};

    #[test]
    fn pressure_switch_configuration_is_required_and_round_trips_on_the_wire() {
        for activation in ["auto", "momentary", "toggle"] {
            for trigger in ["never", "solo", "any", "all"] {
                let value = serde_json::json!({
                    "id": "cyan", "color": "#30d8ff",
                    "pressure_switch": {"activation": activation, "reset_on_player_death": trigger}
                });
                let kind: KindDef = serde_json::from_value(value).expect("valid pressure switch config rejected");
                let bytes = bincode::encode_to_vec(&kind, bincode::config::standard()).expect("kind encoding failed");
                let (decoded, _): (KindDef, _) =
                    bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("kind decoding failed");
                assert_eq!(decoded, kind);
            }
        }
        assert!(serde_json::from_value::<KindDef>(serde_json::json!({"id": "cyan", "color": "#30d8ff"})).is_err());
        for config in [
            serde_json::json!({}),
            serde_json::json!({"activation": "auto"}),
            serde_json::json!({"reset_on_player_death": "all"}),
            serde_json::json!({"activation": "hold", "reset_on_player_death": "all"}),
            serde_json::json!({"activation": "auto", "reset_on_player_death": "always"}),
            serde_json::json!({"activation": "auto", "reset_on_player_death": "single"}),
        ] {
            assert!(
                serde_json::from_value::<KindDef>(serde_json::json!({
                    "id": "cyan", "color": "#30d8ff", "pressure_switch": config
                }))
                .is_err()
            );
        }
    }

    #[test]
    fn rejects_duplicate_ids() {
        let err = BarrierKindTable::from_ids(vec!["a".into(), "a".into()]).expect_err("duplicate ids loaded");
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn rejects_empty_id() {
        let err = BarrierKindTable::from_ids(vec!["a".into(), "".into()]).expect_err("empty id loaded");
        assert!(err.to_string().contains("empty"));
    }

    fn barrier_max() -> usize {
        BarrierKindId::MAX.expect("barrier kinds carry no collision-group cap")
    }

    #[test]
    fn rejects_more_than_max_kinds() {
        let too_many: Vec<String> = (0..=barrier_max()).map(|i| format!("k{i}")).collect();
        let err = BarrierKindTable::from_ids(too_many).expect_err("over-max kinds loaded");
        assert!(err.to_string().contains("barrier_kinds has"));
        assert!(err.to_string().contains("max is"));
    }

    #[test]
    fn accepts_exactly_max_barrier_kinds() {
        let barriers: Vec<String> = (0..barrier_max()).map(|i| format!("k{i}")).collect();
        BarrierKindTable::from_ids(barriers).expect("BarrierKindId::MAX kinds rejected");
    }

    #[test]
    fn bridge_kinds_have_no_group_cap() {
        assert_eq!(BridgeKindId::MAX, None);
        let bridges: Vec<String> = (0..=barrier_max()).map(|i| format!("k{i}")).collect();
        BridgeKindTable::from_ids(bridges).expect("bridge kinds past the barrier cap rejected");
    }

    #[test]
    fn round_trip_index_and_id() {
        let table =
            BarrierKindTable::from_ids(vec!["basement".into(), "boss_room".into()]).expect("two-kind table rejected");
        assert_eq!(table.index_of("basement"), Some(BarrierKindId(0)));
        assert_eq!(table.index_of("boss_room"), Some(BarrierKindId(1)));
        assert_eq!(table.index_of("unknown"), None);
        assert_eq!(table.id(BarrierKindId(0)), Some("basement"));
        assert_eq!(table.id(BarrierKindId(1)), Some("boss_room"));
        assert_eq!(table.id(BarrierKindId(2)), None);
    }

    #[test]
    fn resolve_names_the_table_noun() {
        let table = BridgeKindTable::from_ids(vec!["skyway".into()]).expect("one-kind table rejected");
        assert_eq!(
            table.resolve("skyway").expect("registered kind unresolved"),
            BridgeKindId(0)
        );
        let err = table.resolve("void").expect_err("unregistered kind resolved");
        assert!(err.to_string().contains("unknown bridge kind"), "{err}");
    }
}
