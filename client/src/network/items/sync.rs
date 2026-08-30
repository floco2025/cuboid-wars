use bevy::prelude::*;
use std::collections::HashSet;

use super::super::context::ServerMessageContext;
use crate::items::{ItemInfo, spawn_item};
use common::protocol::*;

// Synchronize items from a snapshot — spawn/despawn.
pub(in crate::network) fn sync_items(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    server_items: &[(ItemId, Item)],
) {
    let server_item_ids: HashSet<ItemId> = server_items.iter().map(|(id, _)| *id).collect();

    // Spawn any items that appear in the snapshot but are missing locally
    for (item_id, item) in server_items {
        if context.items.contains_key(item_id) {
            continue;
        }
        let entity = spawn_item(
            commands,
            &context.item_assets,
            &context.barrier_assets,
            &context.missile_assets,
            *item_id,
            item.item_type,
            &item.pos,
        );
        context.items.insert(*item_id, ItemInfo { entity });
    }

    // Despawn items no longer present in the authoritative snapshot
    context.items.retain(|id, item_info| {
        if server_item_ids.contains(id) {
            true
        } else {
            commands.entity(item_info.entity).despawn();
            false
        }
    });
}
