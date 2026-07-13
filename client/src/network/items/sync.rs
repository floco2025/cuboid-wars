use bevy::prelude::*;
use std::collections::HashSet;

use crate::{
    barriers::BarrierAssets,
    config::AssetSet,
    items::{ItemAssets, ItemInfo, ItemMap, spawn_item},
    players::PlayerMap,
};
use common::protocol::*;

// ============================================================================
// Item Message Handlers
// ============================================================================

// Cookie pickup: play sound + apply the early score for HUD reaction. The
// snapshot will confirm `score` next tick; this is just the latency cut.
pub fn handle_item_collected_message(
    commands: &mut Commands,
    msg: SCookieCollected,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    players: &mut PlayerMap,
    my_player_id: PlayerId,
) {
    if let Some(info) = players.get_mut(&my_player_id) {
        info.score = msg.score;
    }
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.player_sound("collect_cookie").to_owned())),
        PlaybackSettings::DESPAWN,
    ));
}

// Health potion pickup: play sound + apply the early Health for the HUD bar.
// The snapshot will confirm `Health` next tick; this is just the latency cut.
pub fn handle_health_potion_collected_message(
    commands: &mut Commands,
    msg: SHealthPotionCollected,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    players: &PlayerMap,
    my_player_id: PlayerId,
) {
    if let Some(info) = players.get(&my_player_id) {
        commands.entity(info.entity).insert(msg.health);
    }
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.player_sound("collect_power_up").to_owned())),
        PlaybackSettings::DESPAWN,
    ));
}

// ============================================================================
// Item Synchronization Helper
// ============================================================================

// Synchronize items from a snapshot — spawn/despawn.
pub fn sync_items(
    commands: &mut Commands,
    item_assets: &ItemAssets,
    barrier_assets: &BarrierAssets,
    items: &mut ResMut<ItemMap>,
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
