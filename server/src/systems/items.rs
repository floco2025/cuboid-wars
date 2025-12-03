use bevy::prelude::*;
use rand::Rng as _;
use std::collections::HashSet;

use crate::{
    constants::*,
    map::{cell_center, find_unoccupied_cell, grid_coords_from_position},
    net::ServerToClient,
    resources::{ItemInfo, ItemMap, ItemSpawner, PlayerMap},
};
use common::{
    collision::check_player_item_overlap,
    constants::{COOKIE_POINTS, COOKIE_RESPAWN_TIME, GRID_COLS, GRID_ROWS},
    protocol::{ItemId, ItemType, PlayerId, Position, SCookieCollected, SPlayerStatus, ServerMessage},
};

use super::network::broadcast_to_all;

fn choose_item_type(rng: &mut rand::rngs::ThreadRng) -> ItemType {
    let rand_val = rng.random::<f64>();
    if rand_val < 0.33 {
        ItemType::SpeedPowerUp
    } else if rand_val < 0.67 {
        ItemType::MultiShotPowerUp
    } else {
        ItemType::ReflectPowerUp
    }
}

// ============================================================================
// Item Spawn/Despawn Systems
// ============================================================================

// System to spawn cookies on all grid cells at startup
pub fn item_initial_spawn_system(
    mut commands: Commands,
    mut spawner: ResMut<ItemSpawner>,
    mut items: ResMut<ItemMap>,
    query: Query<&ItemId>,
) {
    // Only spawn cookies once - check if any cookies exist
    let has_cookies = query.iter().any(|id| {
        items.0.get(id).is_some_and(|info| info.item_type == ItemType::Cookie)
    });

    if has_cookies {
        return;
    }

    // Spawn one cookie on each grid cell
    for grid_z in 0..GRID_ROWS {
        for grid_x in 0..GRID_COLS {
            let item_id = ItemId(spawner.next_id);
            spawner.next_id += 1;
            let position = cell_center(grid_x, grid_z);

            let entity = commands.spawn((item_id, position)).id();

            items.0.insert(
                item_id,
                ItemInfo {
                    entity,
                    item_type: ItemType::Cookie,
                    spawn_time: 0.0, // Cookie is available (not respawning)
                },
            );
        }
    }
}

// System to spawn items at regular intervals
pub fn item_spawn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut spawner: ResMut<ItemSpawner>,
    mut items: ResMut<ItemMap>,
    positions: Query<&Position>,
) {
    let delta = time.delta_secs();
    spawner.timer += delta;

    if spawner.timer >= ITEM_SPAWN_INTERVAL {
        spawner.timer = 0.0;

        // Get occupied grid cells from existing items
        let occupied_cells: HashSet<(i32, i32)> = items
            .0
            .values()
            .filter_map(|info| positions.get(info.entity).ok().map(grid_coords_from_position))
            .collect();

        let mut rng = rand::rng();

        if let Some((grid_x, grid_z)) = find_unoccupied_cell(&mut rng, &occupied_cells) {
            let item_id = ItemId(spawner.next_id);
            spawner.next_id += 1;
            let position = cell_center(grid_x, grid_z);
            let item_type = choose_item_type(&mut rng);

            let entity = commands.spawn((item_id, position)).id();

            items.0.insert(
                item_id,
                ItemInfo {
                    entity,
                    item_type,
                    spawn_time: time.elapsed_secs(),
                },
            );
        }
    }
}

// System to despawn old items
pub fn item_despawn_system(mut commands: Commands, time: Res<Time>, mut items: ResMut<ItemMap>) {
    let current_time = time.elapsed_secs();

    // Collect items to remove (skip cookies - they respawn instead)
    let items_to_remove: Vec<ItemId> = items
        .0
        .iter()
        .filter(|(_, info)| {
            info.item_type != ItemType::Cookie
                && current_time - info.spawn_time >= ITEM_LIFETIME
        })
        .map(|(id, _)| *id)
        .collect();

    // Remove expired items
    for item_id in items_to_remove {
        if let Some(info) = items.0.remove(&item_id) {
            commands.entity(info.entity).despawn();
        }
    }
}

// ============================================================================
// Item Collection System
// ============================================================================

// System to detect player-item collisions and grant items
pub fn item_collection_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    mut items: ResMut<ItemMap>,
    player_positions: Query<&Position, With<PlayerId>>,
    item_positions: Query<&Position, With<ItemId>>,
) {
    // Check each item against each player
    let items_to_collect: Vec<(PlayerId, ItemId, ItemType)> = items
        .0
        .iter()
        .filter_map(|(item_id, item_info)| {
            let item_pos = item_positions.get(item_info.entity).ok()?;

            // Check against all players
            for (player_id, player_info) in &players.0 {
                if let Ok(player_pos) = player_positions.get(player_info.entity)
                    && check_player_item_overlap(player_pos, item_pos, ITEM_COLLECTION_RADIUS)
                {
                    return Some((*player_id, *item_id, item_info.item_type));
                }
            }
            None
        })
        .collect();

    // Process collections
    let mut power_up_messages = Vec::new();

    for (player_id, item_id, item_type) in items_to_collect {
        // Remove the item from the map
        if let Some(item_info) = items.0.remove(&item_id) {
            commands.entity(item_info.entity).despawn();
        }

        // Update player's power-up timer or points
        if let Some(player_info) = players.0.get_mut(&player_id) {
            match item_type {
                ItemType::SpeedPowerUp => {
                    player_info.speed_power_up_timer = SPEED_POWER_UP_DURATION;
                }
                ItemType::MultiShotPowerUp => {
                    player_info.multi_shot_power_up_timer = MULTI_SHOT_POWER_UP_DURATION;
                }
                ItemType::ReflectPowerUp => {
                    player_info.reflect_power_up_timer = MULTI_SHOT_POWER_UP_DURATION;
                }
                ItemType::Cookie => {
                    // Give points for cookie
                    player_info.hits += COOKIE_POINTS;
                    // Set spawn_time to respawn countdown
                    if let Some(item_info) = items.0.get_mut(&item_id) {
                        item_info.spawn_time = COOKIE_RESPAWN_TIME;
                    }
                    // Send cookie collection message only to this player
                    let _ = player_info.channel.send(ServerToClient::Send(
                        ServerMessage::CookieCollected(SCookieCollected {}),
                    ));
                    debug!("Player {:?} collected cookie, +{} points", player_id, COOKIE_POINTS);
                    continue; // Skip despawning the cookie
                }
            }

            power_up_messages.push(SPlayerStatus {
                id: player_id,
                speed_power_up: player_info.speed_power_up_timer > 0.0,
                multi_shot_power_up: player_info.multi_shot_power_up_timer > 0.0,
                reflect_power_up: player_info.reflect_power_up_timer > 0.0,
                stunned: player_info.stun_timer > 0.0,
            });

            debug!("Player {:?} collected {:?}", player_id, item_type);
        }
    }

    // Send power-up updates to all clients
    for msg in power_up_messages {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}

// ============================================================================
// Cookie Respawn System
// ============================================================================

// System to handle cookie respawning after collection
pub fn item_respawn_system(time: Res<Time>, mut items: ResMut<ItemMap>) {
    let delta = time.delta_secs();

    for item_info in items.0.values_mut() {
        if item_info.item_type != ItemType::Cookie {
            continue;
        }

        // If spawn_time > 0, it's counting down to respawn
        if item_info.spawn_time > 0.0 {
            item_info.spawn_time -= delta;
            if item_info.spawn_time <= 0.0 {
                item_info.spawn_time = 0.0; // Cookie has respawned
                debug!("Cookie respawned");
            }
        }
    }
}
