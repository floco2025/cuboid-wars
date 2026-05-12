use bevy::prelude::*;

use crate::{
    constants::{
        COOKIE_POINTS, COOKIE_RESPAWN_TIME, ITEM_COLLECTION_RADIUS, KEY_RESPAWN_TIME, POWER_UP_ANTI_GRAVITY_DURATION,
        POWER_UP_MULTI_SHOT_DURATION, POWER_UP_PHASING_DURATION, POWER_UP_SPEED_DURATION,
    },
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
                        return Some((*player_id, *item_id, item_info.item_type));
                    }
                }
            }
            None
        })
        .collect();

    let mut power_up_messages = Vec::new();

    for (player_id, item_id, item_type) in items_to_collect {
        if item_type == ItemType::Cookie {
            if let Some(player_info) = players.get_mut(&player_id) {
                player_info.hits += COOKIE_POINTS;

                if let Some(item_info) = items.get_mut(&item_id) {
                    item_info.spawn_time = COOKIE_RESPAWN_TIME;
                }

                let _ = player_info
                    .channel
                    .send(ServerToClient::Send(ServerMessage::CookieCollected(
                        SCookieCollected {},
                    )));
            }
            continue;
        }

        if let ItemType::Key(kind) = item_type {
            if let Some(item_info) = items.get_mut(&item_id) {
                item_info.spawn_time = KEY_RESPAWN_TIME;
            }
            if let Some(player_info) = players.get_mut(&player_id) {
                // `add_key` returns true only on a state change — that's the
                // gate for re-broadcasting `SPlayerStatus`. Picking up a key
                // you already hold is a no-op, no broadcast.
                if player_info.add_key(kind) {
                    power_up_messages.push(player_info.status(player_id));
                }
            }
            continue;
        }

        if let Some(item_info) = items.remove(&item_id) {
            commands.entity(item_info.entity).despawn();
        }

        if let Some(player_info) = players.get_mut(&player_id) {
            match item_type {
                ItemType::SpeedPowerUp => {
                    player_info.speed_power_up_timer = POWER_UP_SPEED_DURATION;
                }
                ItemType::MultiShotPowerUp => {
                    player_info.multi_shot_power_up_timer = POWER_UP_MULTI_SHOT_DURATION;
                }
                ItemType::PhasingPowerUp => {
                    player_info.phasing_power_up_timer = POWER_UP_PHASING_DURATION;
                }
                ItemType::AntiGravityPowerUp => {
                    player_info.anti_gravity_power_up_timer = POWER_UP_ANTI_GRAVITY_DURATION;
                }
                ItemType::Cookie | ItemType::Key(_) => unreachable!("handled above"),
            }

            power_up_messages.push(player_info.status(player_id));
        }
    }

    for msg in power_up_messages {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}
