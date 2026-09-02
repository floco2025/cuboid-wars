use std::collections::HashMap;

use bevy::prelude::*;

use common::protocol::{ItemId, ItemType, MapWeaponSettings};

pub enum ItemPlacement {
    // World-spawned by the random spawner; despawns outright on pickup or
    // once `RandomItems::despawn_secs` elapses.
    Random { spawned_at: f32 },
    // Map-authored; hides on pickup and re-shows when the countdown hits 0.
    Placed { respawn_countdown: f32 },
}

pub struct ItemInfo {
    pub entity: Entity,
    pub item_type: ItemType,
    pub placement: ItemPlacement,
}

impl ItemInfo {
    // Hidden items stay out of snapshots and can't be collected; the entity
    // persists so the item keeps its cell (and its occupancy claim).
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        matches!(self.placement, ItemPlacement::Placed { respawn_countdown } if respawn_countdown > 0.0)
    }
}

// Resolved `maps.<name>.random_items` from the server gameplay config,
// parsed once at startup. An empty pool means no random spawning.
#[derive(Resource, Clone, Default)]
pub struct RandomItems {
    pub pool: Vec<ItemType>,
    pub max_number: usize,
    pub despawn_secs: f32,
}

impl RandomItems {
    #[must_use]
    pub fn from_config(config: Option<&crate::config::RandomItemsConfig>, weapons: MapWeaponSettings) -> Self {
        config.map_or_else(Self::default, |random_items_config| Self {
            pool: random_items_config
                .types
                .iter()
                .map(|id| {
                    ItemType::from_config_id(id)
                        .expect("random item type missing from ItemType config ids after config validation")
                })
                .filter(|item_type| weapons.allows_item(*item_type))
                .collect(),
            max_number: random_items_config.max_number,
            despawn_secs: random_items_config.despawn_secs,
        })
    }
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
