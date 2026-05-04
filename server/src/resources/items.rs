use std::collections::HashMap;

use bevy::prelude::*;

use common::protocol::{ItemId, ItemType};

pub struct ItemInfo {
    pub entity: Entity,
    pub item_type: ItemType,
    pub spawn_time: f32,
}

#[derive(Resource, Default)]
pub struct ItemMap(pub HashMap<ItemId, ItemInfo>);

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
