use bevy::prelude::*;
use std::collections::HashSet;

use crate::{
    resources::{ItemInfo, ItemMap},
    spawning::spawn_item,
};
use common::protocol::*;

// ============================================================================
// Item Message Handlers
// ============================================================================

// Handle item collected message - play sound effect.
pub fn handle_item_collected_message(commands: &mut Commands, _msg: SCookieCollected, asset_server: &AssetServer) {
    // Play sound - this message is only sent to the player who collected it
    commands.spawn((
        AudioPlayer::new(asset_server.load("sounds/player_cookie.ogg")),
        PlaybackSettings::DESPAWN,
    ));
}

// ============================================================================
// Item Synchronization Helper
// ============================================================================

// Synchronize items from bulk Update message - spawn/despawn.
pub fn sync_items(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    items: &mut ResMut<ItemMap>,
    asset_server: &Res<AssetServer>,
    server_items: &[(ItemId, Item)],
) {
    let server_item_ids: HashSet<ItemId> = server_items.iter().map(|(id, _)| *id).collect();

    // Spawn any items that appear in the update but are missing locally
    for (item_id, item) in server_items {
        if items.0.contains_key(item_id) {
            continue;
        }
        let entity = spawn_item(
            commands,
            meshes,
            materials,
            asset_server,
            *item_id,
            item.item_type,
            &item.pos,
        );
        items.0.insert(*item_id, ItemInfo { entity });
    }

    // Despawn items no longer present in the authoritative snapshot
    let stale_item_ids: Vec<ItemId> = items
        .0
        .keys()
        .filter(|id| !server_item_ids.contains(id))
        .copied()
        .collect();

    for item_id in stale_item_ids {
        if let Some(item_info) = items.0.remove(&item_id) {
            commands.entity(item_info.entity).despawn();
        }
    }
}
