use bevy::prelude::*;

use crate::{
    config::ServerGameplayConfig,
    net::ServerToClient,
    network::broadcast_to_all,
    resources::{ItemMap, ItemPlacement, PlayerMap, QuestEvent, record_quest_event},
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
            // A placed item counting down its respawn exists server-side but
            // is invisible and uncollectable until the timer elapses.
            if item_info.is_hidden() {
                return None;
            }

            let item_pos = item_positions.get(item_info.entity).ok()?;

            for (player_id, player_info) in players.iter() {
                // A killed player's entity despawn is deferred, so a same-tick
                // corpse still overlaps items — and a key collected here would
                // land after `clear_per_life_state`, surviving into the next life.
                if player_info.is_dead() {
                    continue;
                }
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
        consume_item(&mut commands, &mut items, &server_gameplay_config, item_id, item_type);
        match item_type {
            ItemType::Cookie => collect_cookie(&mut players, player_id, &server_gameplay_config),
            ItemType::Key(kind) => collect_key(&mut players, player_id, kind, &mut status_broadcasts),
            ItemType::HealthPotion => collect_health_potion(
                &mut players,
                &mut player_health,
                player_id,
                &server_gameplay_config,
                &gameplay_config,
            ),
            ItemType::SpeedPowerUp
            | ItemType::MultiShotPowerUp
            | ItemType::PhasingPowerUp
            | ItemType::LowGravityPowerUp => {
                // Guarded by the enum arm so an item type whose taxonomy
                // changes won't silently fall through to a power-up handler.
                assert!(item_type.is_timer_power_up());
                collect_power_up(
                    &mut players,
                    player_id,
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

// Hide-and-rearm for placed items, full despawn for random ones. The
// per-type handlers only apply effects.
fn consume_item(
    commands: &mut Commands,
    items: &mut ItemMap,
    server_gameplay_config: &ServerGameplayConfig,
    item_id: ItemId,
    item_type: ItemType,
) {
    let Some(item_info) = items.get_mut(&item_id) else {
        return;
    };
    if let ItemPlacement::Placed { respawn_countdown } = &mut item_info.placement {
        *respawn_countdown = server_gameplay_config.placed_items.respawn_secs_for(item_type);
        return;
    }
    if let Some(info) = items.remove(&item_id) {
        commands.entity(info.entity).despawn();
    }
}

fn collect_cookie(players: &mut PlayerMap, player_id: PlayerId, server_gameplay_config: &ServerGameplayConfig) {
    let Some(player_info) = players.get_mut(&player_id) else {
        return;
    };
    player_info.score += server_gameplay_config.scoring.cookie;
    let quest_messages = record_quest_event(player_info, &server_gameplay_config.quests, QuestEvent::CookieCollected);
    let _ = player_info
        .channel
        .send(ServerToClient::Send(ServerMessage::CookieCollected(SCookieCollected {
            score: player_info.score,
        })));
    for msg in quest_messages {
        let _ = player_info.channel.send(ServerToClient::Send(msg));
    }
}

fn collect_key(
    players: &mut PlayerMap,
    player_id: PlayerId,
    kind: common::protocol::BarrierKindId,
    status_broadcasts: &mut Vec<common::protocol::SPlayerStatus>,
) {
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
    players: &mut PlayerMap,
    player_health: &mut Query<&mut Health, With<PlayerMarker>>,
    player_id: PlayerId,
    server_gameplay_config: &ServerGameplayConfig,
    gameplay_config: &GameplayConfig,
) {
    let Some(player_info) = players.get(&player_id) else {
        return;
    };
    let Ok(mut health) = player_health.get_mut(player_info.entity) else {
        return;
    };
    let max = gameplay_config.player.health().max;
    let heal = max * server_gameplay_config.power_ups.health_potion_heal_fraction;
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
    players: &mut PlayerMap,
    player_id: PlayerId,
    item_type: ItemType,
    server_gameplay_config: &ServerGameplayConfig,
    status_broadcasts: &mut Vec<common::protocol::SPlayerStatus>,
) {
    let Some(player_info) = players.get_mut(&player_id) else {
        return;
    };
    player_info.grant_power_up(item_type, &server_gameplay_config.power_ups);
    status_broadcasts.push(player_info.status(player_id));
}
