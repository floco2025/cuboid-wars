use bevy::prelude::*;

use crate::{
    config::ServerGameplayConfig,
    net::ServerToClient,
    network::broadcast_to_all,
    resources::{ItemMap, PlayerMap, record_cookie_for_quests},
};
use common::{
    config::GameplayConfig,
    health::regenerate_health,
    physics::character_overlaps_item,
    protocol::{
        Health, ItemId, ItemMarker, ItemType, PlayerId, PlayerMarker, Position, SCookieCollected,
        SHealthPotionCollected, ServerMessage,
    },
};

const ITEM_COLLECTION_RADIUS: f32 = 1.0;
const ITEM_PICKUP_FLOOR_EPSILON: f32 = 0.1;

pub fn item_collection_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    mut items: ResMut<ItemMap>,
    character_positions: Query<&Position, With<PlayerMarker>>,
    mut player_health: Query<&mut Health, With<PlayerMarker>>,
    item_positions: Query<&Position, With<ItemMarker>>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    gameplay_config: Res<GameplayConfig>,
) {
    let items_to_collect: Vec<(PlayerId, ItemId, ItemType)> = items
        .iter()
        .filter_map(|(item_id, item_info)| {
            // Items that use the `spawn_time` countdown as a "currently
            // respawning, invisible" flag — cookies + keys. Skip until the
            // timer has elapsed. Power-ups + potions despawn fully on
            // pickup so they never hit this branch.
            if item_info.item_type.respects_respawn_timer() && item_info.spawn_time > 0.0 {
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
                &server_gameplay_config,
                &mut status_broadcasts,
            ),
            ItemType::HealthPotion => collect_health_potion(
                &mut commands,
                &mut players,
                &mut items,
                &mut player_health,
                player_id,
                item_id,
                &server_gameplay_config,
                &gameplay_config,
            ),
            ItemType::SpeedPowerUp
            | ItemType::MultiShotPowerUp
            | ItemType::PhasingPowerUp
            | ItemType::AntiGravityPowerUp => {
                // Guarded by the enum arm so an item type whose taxonomy
                // changes won't silently fall through to a power-up handler.
                assert!(item_type.is_timer_power_up());
                collect_power_up(
                    &mut commands,
                    &mut players,
                    &mut items,
                    player_id,
                    item_id,
                    item_type,
                    &server_gameplay_config,
                    &mut status_broadcasts,
                );
            }
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
        item_info.spawn_time = server_gameplay_config.cookies.respawn_secs;
    }
    let achievements = record_cookie_for_quests(player_info, &server_gameplay_config.quests);
    let _ = player_info
        .channel
        .send(ServerToClient::Send(ServerMessage::CookieCollected(SCookieCollected {
            score: player_info.score,
        })));
    for msg in achievements {
        let _ = player_info
            .channel
            .send(ServerToClient::Send(ServerMessage::QuestAchieved(msg)));
    }
}

fn collect_key(
    players: &mut PlayerMap,
    items: &mut ItemMap,
    player_id: PlayerId,
    item_id: ItemId,
    kind: common::protocol::BarrierKindId,
    server_gameplay_config: &ServerGameplayConfig,
    status_broadcasts: &mut Vec<common::protocol::SPlayerStatus>,
) {
    if let Some(item_info) = items.get_mut(&item_id) {
        item_info.spawn_time = server_gameplay_config.keys.respawn_secs;
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

fn collect_health_potion(
    commands: &mut Commands,
    players: &mut PlayerMap,
    items: &mut ItemMap,
    player_health: &mut Query<&mut Health, With<PlayerMarker>>,
    player_id: PlayerId,
    item_id: ItemId,
    server_gameplay_config: &ServerGameplayConfig,
    gameplay_config: &GameplayConfig,
) {
    if let Some(item_info) = items.remove(&item_id) {
        commands.entity(item_info.entity).despawn();
    }
    let Some(player_info) = players.get(&player_id) else {
        return;
    };
    let Ok(mut health) = player_health.get_mut(player_info.entity) else {
        return;
    };
    let max = gameplay_config.player.health().max;
    let heal = max * server_gameplay_config.power_ups.health_potion_heal_percent;
    regenerate_health(&mut health, max, heal);
    // Unicast pickup cue — carries the post-heal value so the HUD bumps on
    // the pickup tick instead of waiting for the next snapshot.
    let _ = player_info
        .channel
        .send(ServerToClient::Send(ServerMessage::HealthPotionCollected(
            SHealthPotionCollected { health: *health },
        )));
}

fn collect_power_up(
    commands: &mut Commands,
    players: &mut PlayerMap,
    items: &mut ItemMap,
    player_id: PlayerId,
    item_id: ItemId,
    item_type: ItemType,
    server_gameplay_config: &ServerGameplayConfig,
    status_broadcasts: &mut Vec<common::protocol::SPlayerStatus>,
) {
    if let Some(item_info) = items.remove(&item_id) {
        commands.entity(item_info.entity).despawn();
    }
    let Some(player_info) = players.get_mut(&player_id) else {
        return;
    };
    player_info.grant_power_up(item_type, &server_gameplay_config.power_ups);
    status_broadcasts.push(player_info.status(player_id));
}
