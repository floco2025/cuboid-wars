use std::collections::HashMap;

use anyhow::{Result, bail};
use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};

// Rapier supports 32 collision groups via a u32 bitmask. Walls/floors/ramps
// reserve groups 1/2/3 (bits 0/1/2). The remaining 29 are available for
// barrier kinds — more than enough for any reasonable game.
pub const BARRIER_KIND_MAX: usize = 29;

// Stable on-wire index into the map file's ordered `barrier_kinds`. The server
// ships that list in `SInit` so both sides assign the same indices.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct BarrierKindId(pub u16);

// Identity-only table: maps `BarrierKindId` ↔ `id` string. The client builds
// a parallel `BarrierKindColors` resource at startup; the server doesn't
// need colors at all.
#[derive(Resource, Debug, Clone, Default)]
pub struct BarrierKindTable {
    ids: Vec<String>,
    index_by_id: HashMap<String, BarrierKindId>,
}

impl BarrierKindTable {
    pub fn from_ids(ids: Vec<String>) -> Result<Self> {
        if ids.len() > BARRIER_KIND_MAX {
            bail!(
                "barrier_kinds has {} entries; max is {} (limited by available Rapier collision groups)",
                ids.len(),
                BARRIER_KIND_MAX
            );
        }
        let mut index_by_id = HashMap::with_capacity(ids.len());
        for (idx, id) in ids.iter().enumerate() {
            if id.is_empty() {
                bail!("barrier_kinds[{idx}] is empty");
            }
            let kind = BarrierKindId(
                u16::try_from(idx)
                    .map_err(|_| anyhow::anyhow!("barrier_kinds exceeds {} entries (u16 index overflow)", u16::MAX))?,
            );
            if index_by_id.insert(id.clone(), kind).is_some() {
                bail!("barrier_kinds contains duplicate id {id:?}");
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
    pub fn index_of(&self, id: &str) -> Option<BarrierKindId> {
        self.index_by_id.get(id).copied()
    }

    #[must_use]
    pub fn id(&self, kind: BarrierKindId) -> Option<&str> {
        self.ids.get(kind.0 as usize).map(String::as_str)
    }

    // Resolve a string id, returning a helpful error if it isn't registered.
    pub fn resolve(&self, id: &str) -> Result<BarrierKindId> {
        self.index_of(id).ok_or_else(|| {
            let known = self.ids.join(", ");
            anyhow::anyhow!("unknown barrier kind {id:?}; known kinds: [{known}]")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_ids() {
        let err = BarrierKindTable::from_ids(vec!["a".into(), "a".into()]).expect_err("duplicate ids should fail");
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn rejects_empty_id() {
        let err = BarrierKindTable::from_ids(vec!["a".into(), "".into()]).expect_err("empty id should fail");
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn rejects_more_than_max_kinds() {
        let too_many: Vec<String> = (0..BARRIER_KIND_MAX + 1).map(|i| format!("k{i}")).collect();
        let err = BarrierKindTable::from_ids(too_many).expect_err("over-max kinds should fail");
        assert!(err.to_string().contains("max is"));
    }

    #[test]
    fn accepts_exactly_max_kinds() {
        let exact: Vec<String> = (0..BARRIER_KIND_MAX).map(|i| format!("k{i}")).collect();
        BarrierKindTable::from_ids(exact).expect("BARRIER_KIND_MAX kinds should load");
    }

    #[test]
    fn round_trip_index_and_id() {
        let table = BarrierKindTable::from_ids(vec!["basement".into(), "boss_room".into()])
            .expect("two-kind table should load");
        assert_eq!(table.index_of("basement"), Some(BarrierKindId(0)));
        assert_eq!(table.index_of("boss_room"), Some(BarrierKindId(1)));
        assert_eq!(table.index_of("unknown"), None);
        assert_eq!(table.id(BarrierKindId(0)), Some("basement"));
        assert_eq!(table.id(BarrierKindId(1)), Some("boss_room"));
        assert_eq!(table.id(BarrierKindId(2)), None);
    }
}
