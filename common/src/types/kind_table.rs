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
#[path = "tests/kind_table.rs"]
mod tests;
