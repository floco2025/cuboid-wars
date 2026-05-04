use bevy::prelude::*;
use std::collections::HashMap;

use common::protocol::ItemId;

// Item information (client-side).
pub struct ItemInfo {
    pub entity: Entity,
}

// Map of all items (client-side source of truth).
#[derive(Resource, Default)]
pub struct ItemMap(HashMap<ItemId, ItemInfo>);

impl ItemMap {
    pub fn insert(&mut self, id: ItemId, info: ItemInfo) -> Option<ItemInfo> {
        self.0.insert(id, info)
    }

    #[must_use]
    pub fn contains_key(&self, id: &ItemId) -> bool {
        self.0.contains_key(id)
    }

    pub fn retain(&mut self, f: impl FnMut(&ItemId, &mut ItemInfo) -> bool) {
        self.0.retain(f);
    }
}
