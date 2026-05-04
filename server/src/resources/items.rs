use std::collections::HashMap;

use bevy::prelude::*;

use common::protocol::{ItemId, ItemType};

pub struct ItemInfo {
    pub entity: Entity,
    pub item_type: ItemType,
    pub spawn_time: f32,
}

#[derive(Resource, Default)]
pub struct ItemMap(HashMap<ItemId, ItemInfo>);

impl ItemMap {
    pub fn insert(&mut self, id: ItemId, info: ItemInfo) -> Option<ItemInfo> {
        self.0.insert(id, info)
    }

    pub fn remove(&mut self, id: &ItemId) -> Option<ItemInfo> {
        self.0.remove(id)
    }

    #[must_use]
    pub fn get(&self, id: &ItemId) -> Option<&ItemInfo> {
        self.0.get(id)
    }

    pub fn get_mut(&mut self, id: &ItemId) -> Option<&mut ItemInfo> {
        self.0.get_mut(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ItemId, &ItemInfo)> {
        self.0.iter()
    }

    pub fn values(&self) -> impl Iterator<Item = &ItemInfo> {
        self.0.values()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut ItemInfo> {
        self.0.values_mut()
    }
}

#[derive(Resource)]
pub struct ItemSpawner {
    pub timer: f32,
    pub next_id: u32,
}

impl Default for ItemSpawner {
    fn default() -> Self {
        Self { timer: 0.0, next_id: 0 }
    }
}
