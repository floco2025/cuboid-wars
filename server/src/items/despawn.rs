use bevy::prelude::*;

use crate::{constants::ITEM_LIFETIME, resources::ItemMap};
use common::protocol::{ItemId, ItemType};

pub fn item_despawn_system(mut commands: Commands, time: Res<Time>, mut items: ResMut<ItemMap>) {
    let current_time = time.elapsed_secs();

    // Only power-ups expire from inactivity. Cookies and keys are persistent
    // world items managed by the respawn tick.
    let items_to_remove: Vec<ItemId> = items
        .iter()
        .filter(|(_, info)| {
            !matches!(info.item_type, ItemType::Cookie | ItemType::Key(_))
                && current_time - info.spawn_time >= ITEM_LIFETIME
        })
        .map(|(id, _)| *id)
        .collect();

    for item_id in items_to_remove {
        if let Some(info) = items.remove(&item_id) {
            commands.entity(info.entity).despawn();
        }
    }
}

pub fn item_respawn_system(time: Res<Time>, mut items: ResMut<ItemMap>) {
    let delta = time.delta_secs();

    for item_info in items.values_mut() {
        // Cookies and keys both run their respawn countdown here. Power-ups
        // skip — they're despawned outright when their lifetime ends.
        if !matches!(item_info.item_type, ItemType::Cookie | ItemType::Key(_)) {
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
