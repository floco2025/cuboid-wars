use bevy::prelude::*;
use std::collections::HashSet;

use crate::{
    config::AssetSet,
    items::{ItemAssets, ItemInfo, ItemMap, spawn_item},
};
use common::protocol::*;

// ============================================================================
// Item Message Handlers
// ============================================================================

// Handle item collected message - play sound effect.
pub fn handle_item_collected_message(
    commands: &mut Commands,
    _msg: SCookieCollected,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
) {
    // Play sound - this message is only sent to the player who collected it
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.player_sound("collect_cookie").to_owned())),
        PlaybackSettings::DESPAWN,
    ));
}

// ============================================================================
// Item Synchronization Helper
// ============================================================================

// Synchronize items from bulk Update message - spawn/despawn.
pub fn sync_items(
    commands: &mut Commands,
    item_assets: &ItemAssets,
    items: &mut ResMut<ItemMap>,
    server_items: &[(ItemId, Item)],
) {
    let server_item_ids: HashSet<ItemId> = server_items.iter().map(|(id, _)| *id).collect();

    // Spawn any items that appear in the update but are missing locally
    for (item_id, item) in server_items {
        if items.contains_key(item_id) {
            continue;
        }
        let entity = spawn_item(commands, item_assets, *item_id, item.item_type, &item.pos);
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
