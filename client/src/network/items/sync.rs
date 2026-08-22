use bevy::prelude::*;
use std::collections::HashSet;

use crate::{
    barriers::BarrierAssets,
    items::{ItemAssets, ItemInfo, ItemMap, spawn_item},
    missiles::MissileAssets,
};
use common::protocol::*;

// ============================================================================
// Item Synchronization Helper
// ============================================================================

// Synchronize items from a snapshot — spawn/despawn.
pub fn sync_items(
    commands: &mut Commands,
    item_assets: &ItemAssets,
    barrier_assets: &BarrierAssets,
    missile_assets: &MissileAssets,
    items: &mut ItemMap,
    server_items: &[(ItemId, Item)],
) {
    let server_item_ids: HashSet<ItemId> = server_items.iter().map(|(id, _)| *id).collect();

    // Spawn any items that appear in the snapshot but are missing locally
    for (item_id, item) in server_items {
        if items.contains_key(item_id) {
            continue;
        }
        let entity = spawn_item(
            commands,
            item_assets,
            barrier_assets,
            missile_assets,
            *item_id,
            item.item_type,
            &item.pos,
        );
        items.insert(*item_id, ItemInfo { entity });
    }

    // Despawn items no longer present in the authoritative snapshot
    items.retain(|id, item_info| {
        if server_item_ids.contains(id) {
            true
        } else {
            commands.entity(item_info.entity).despawn();
            false
        }
    });
}
