use bevy::prelude::*;

use crate::{
    config::ServerGameplayConfig,
    constants::{COOKIE_RESPAWN_TIME, ITEM_COLLECTION_RADIUS, KEY_RESPAWN_TIME},
    net::ServerToClient,
    network::broadcast_to_all,
    resources::{ItemMap, PlayerMap},
};
use common::{
    physics::character_overlaps_item,
    protocol::{ItemId, ItemMarker, ItemType, PlayerId, PlayerMarker, Position, SCookieCollected, ServerMessage},
};

const ITEM_PICKUP_FLOOR_EPSILON: f32 = 0.1;

pub fn item_collection_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    mut items: ResMut<ItemMap>,
    character_positions: Query<&Position, With<PlayerMarker>>,
    item_positions: Query<&Position, With<ItemMarker>>,
    server_gameplay_config: Res<ServerGameplayConfig>,
) {
    let items_to_collect: Vec<(PlayerId, ItemId, ItemType)> = items
        .iter()
        .filter_map(|(item_id, item_info)| {
            // Cookies and keys both use the `spawn_time` countdown as a
            // "currently respawning, invisible" flag. Don't allow collection
            // until the timer has elapsed.
            if matches!(item_info.item_type, ItemType::Cookie | ItemType::Key(_)) && item_info.spawn_time > 0.0 {
                return None;
            }

            let item_pos = item_positions.get(item_info.entity).ok()?;

            for (player_id, player_info) in players.iter() {
                if let Ok(character_pos) = character_positions.get(player_info.entity) {
                    if (character_pos.y - item_pos.y).abs() > ITEM_PICKUP_FLOOR_EPSILON {
                        continue;
                    }

                    if character_overlaps_item(character_pos, item_pos, ITEM_COLLECTION_RADIUS) {
                        // A player who already holds this kind walks over the
                        // world key without effect. The key stays in place so
                        // another player can still collect it.
                        if let ItemType::Key(kind) = item_info.item_type
                            && player_info.has_key(kind)
                        {
                            continue;
                        }
                        return Some((*player_id, *item_id, item_info.item_type));
                    }
                }
            }
            None
        })
        .collect();

    let mut status_broadcasts = Vec::new();

    for (player_id, item_id, item_type) in items_to_collect {
        match item_type {
            ItemType::Cookie => collect_cookie(&mut players, &mut items, player_id, item_id, &server_gameplay_config),
            ItemType::Key(kind) => collect_key(
                &mut players,
                &mut items,
                player_id,
                item_id,
                kind,
                &mut status_broadcasts,
            ),
            ItemType::SpeedPowerUp
            | ItemType::MultiShotPowerUp
            | ItemType::PhasingPowerUp
            | ItemType::AntiGravityPowerUp => collect_power_up(
                &mut commands,
                &mut players,
                &mut items,
                player_id,
                item_id,
                item_type,
                &mut status_broadcasts,
            ),
        }
    }

    for msg in status_broadcasts {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}

fn collect_cookie(
    players: &mut PlayerMap,
    items: &mut ItemMap,
    player_id: PlayerId,
    item_id: ItemId,
    server_gameplay_config: &ServerGameplayConfig,
) {
    let Some(player_info) = players.get_mut(&player_id) else {
        return;
    };
    player_info.score += server_gameplay_config.scoring.cookie;
    if let Some(item_info) = items.get_mut(&item_id) {
        item_info.spawn_time = COOKIE_RESPAWN_TIME;
    }
    let _ = player_info
        .channel
        .send(ServerToClient::Send(ServerMessage::CookieCollected(
            SCookieCollected {},
        )));
}

fn collect_key(
    players: &mut PlayerMap,
    items: &mut ItemMap,
    player_id: PlayerId,
    item_id: ItemId,
    kind: common::protocol::BarrierKindId,
    status_broadcasts: &mut Vec<common::protocol::SPlayerStatus>,
) {
    if let Some(item_info) = items.get_mut(&item_id) {
        item_info.spawn_time = KEY_RESPAWN_TIME;
    }
    let Some(player_info) = players.get_mut(&player_id) else {
        return;
    };
    // `add_key` returns true only on a state change — that's the gate for
    // re-broadcasting `SPlayerStatus`. Already-held kinds are filtered out
    // by the overlap pass before we get here, but be defensive.
    if player_info.add_key(kind) {
        status_broadcasts.push(player_info.status(player_id));
    }
}

fn collect_power_up(
    commands: &mut Commands,
    players: &mut PlayerMap,
    items: &mut ItemMap,
    player_id: PlayerId,
    item_id: ItemId,
    item_type: ItemType,
    status_broadcasts: &mut Vec<common::protocol::SPlayerStatus>,
) {
    if let Some(item_info) = items.remove(&item_id) {
        commands.entity(item_info.entity).despawn();
    }
    let Some(player_info) = players.get_mut(&player_id) else {
        return;
    };
    player_info.grant_power_up(item_type);
    status_broadcasts.push(player_info.status(player_id));
}
