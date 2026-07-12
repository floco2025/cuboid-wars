use bevy::prelude::*;

use crate::resources::{ItemMap, ItemPlacement, RandomItems};
use common::protocol::ItemId;

pub fn random_item_despawn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut items: ResMut<ItemMap>,
    random_items: Res<RandomItems>,
) {
    let current_time = time.elapsed_secs();

    // Only random items expire from inactivity. Placed items persist and are
    // managed by the respawn tick.
    let items_to_remove: Vec<ItemId> = items
        .iter()
        .filter(|(_, info)| {
            matches!(info.placement, ItemPlacement::Random { spawned_at }
                if current_time - spawned_at >= random_items.despawn_secs)
        })
        .map(|(id, _)| *id)
        .collect();

    for item_id in items_to_remove {
        if let Some(info) = items.remove(&item_id) {
            commands.entity(info.entity).despawn();
        }
    }
}

pub fn placed_item_respawn_system(time: Res<Time>, mut items: ResMut<ItemMap>) {
    let delta = time.delta_secs();

    for item_info in items.values_mut() {
        if let ItemPlacement::Placed { respawn_countdown } = &mut item_info.placement
            && *respawn_countdown > 0.0
        {
            *respawn_countdown = (*respawn_countdown - delta).max(0.0);
        }
    }
}
