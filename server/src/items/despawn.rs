use bevy::prelude::*;

use crate::{constants::ITEM_LIFETIME, resources::ItemMap};
use common::protocol::{ItemId, ItemType};

pub fn item_despawn_system(mut commands: Commands, time: Res<Time>, mut items: ResMut<ItemMap>) {
    let current_time = time.elapsed_secs();

    let items_to_remove: Vec<ItemId> = items
        .0
        .iter()
        .filter(|(_, info)| info.item_type != ItemType::Cookie && current_time - info.spawn_time >= ITEM_LIFETIME)
        .map(|(id, _)| *id)
        .collect();

    for item_id in items_to_remove {
        if let Some(info) = items.0.remove(&item_id) {
            commands.entity(info.entity).despawn();
        }
    }
}

pub fn item_respawn_system(time: Res<Time>, mut items: ResMut<ItemMap>) {
    let delta = time.delta_secs();

    for item_info in items.0.values_mut() {
        if item_info.item_type != ItemType::Cookie {
            continue;
        }

        if item_info.spawn_time > 0.0 {
            item_info.spawn_time -= delta;
            if item_info.spawn_time <= 0.0 {
                item_info.spawn_time = 0.0;
            }
        }
    }
}
