use bevy::prelude::*;
use rand::Rng as _;
use std::collections::HashSet;

use crate::{
    constants::*,
    map::{cell_center, find_unoccupied_cell_not_ramp, grid_coords_from_position},
    net::ServerToClient,
    resources::{GridConfig, ItemInfo, ItemMap, ItemSpawner, PlayerMap},
};
use common::{
    collision::items::overlap_player_vs_item,
    constants::{
        ALWAYS_GHOST_HUNT, ALWAYS_MULTI_SHOT, ALWAYS_PHASING,
        ALWAYS_REFLECT, ALWAYS_SPEED, GRID_COLS, GRID_ROWS,
    },
    markers::{ItemMarker, PlayerMarker},
    protocol::{ItemId, ItemType, PlayerId, Position, SCookieCollected, SPlayerStatus, ServerMessage},
};

use super::network::broadcast_to_all;

// ============================================================================
// Helper Functions
// ============================================================================

fn choose_item_type(rng: &mut rand::rngs::ThreadRng) -> ItemType {
    let rand_val = rng.random::<f64>();
    if rand_val < 0.20 {
        ItemType::SpeedPowerUp
    } else if rand_val < 0.40 {
        ItemType::MultiShotPowerUp
    } else if rand_val < 0.60 {
        ItemType::ReflectPowerUp
    } else if rand_val < 0.80 {
        ItemType::PhasingPowerUp
    } else {
        ItemType::GhostHuntPowerUp
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
    let has_cookies = query
        .iter()
        .any(|id| items.0.get(id).is_some_and(|info| info.item_type == ItemType::Cookie));

    if has_cookies {
        return;
    }

    // Spawn one cookie on each grid cell
    for grid_z in 0..GRID_ROWS {
        for grid_x in 0..GRID_COLS {
            let item_id = ItemId(spawner.next_id);
            spawner.next_id += 1;
            let position = cell_center(grid_x, grid_z);

            let entity = commands.spawn((ItemMarker, item_id, position)).id();

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
    grid_config: Res<GridConfig>,
) {
    let delta = time.delta_secs();
    spawner.timer += delta;

    if spawner.timer >= ITEM_SPAWN_INTERVAL {
        spawner.timer = 0.0;

        // Get occupied grid cells from existing power-ups (ignore cookies)
        let occupied_cells: HashSet<(i32, i32)> = items
            .0
            .values()
            .filter(|info| info.item_type != ItemType::Cookie)
            .filter_map(|info| positions.get(info.entity).ok().map(grid_coords_from_position))
            .collect();

        let mut rng = rand::rng();

        if let Some((grid_x, grid_z)) = find_unoccupied_cell_not_ramp(&mut rng, &occupied_cells, &grid_config.grid) {
            let item_id = ItemId(spawner.next_id);
            spawner.next_id += 1;
            let position = cell_center(grid_x, grid_z);
            let item_type = choose_item_type(&mut rng);

            let entity = commands.spawn((ItemMarker, item_id, position)).id();

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
        .filter(|(_, info)| info.item_type != ItemType::Cookie && current_time - info.spawn_time >= ITEM_LIFETIME)
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
    player_positions: Query<&Position, With<PlayerMarker>>,
    item_positions: Query<&Position, With<ItemMarker>>,
) {
    // Check each item against each player
    let items_to_collect: Vec<(PlayerId, ItemId, ItemType)> = items
        .0
        .iter()
        .filter_map(|(item_id, item_info)| {
            // Skip cookies that are currently respawning
            if item_info.item_type == ItemType::Cookie && item_info.spawn_time > 0.0 {
                return None;
            }

            let item_pos = item_positions.get(item_info.entity).ok()?;

            // Check against all players
            for (player_id, player_info) in &players.0 {
                if let Ok(player_pos) = player_positions.get(player_info.entity)
                    && overlap_player_vs_item(player_pos, item_pos, ITEM_COLLECTION_RADIUS)
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
        // Handle cookies differently - don't despawn, just set respawn timer
        if item_type == ItemType::Cookie {
            if let Some(player_info) = players.0.get_mut(&player_id) {
                // Give points for cookie
                player_info.hits += COOKIE_POINTS;

                // Set spawn_time to respawn countdown
                if let Some(item_info) = items.0.get_mut(&item_id) {
                    item_info.spawn_time = COOKIE_RESPAWN_TIME;
                }

                // Send cookie collection message only to this player
                let _ = player_info
                    .channel
                    .send(ServerToClient::Send(ServerMessage::CookieCollected(
                        SCookieCollected {},
                    )));
            }
            continue; // Don't despawn the cookie
        }

        // Remove non-cookie items from the map and despawn
        if let Some(item_info) = items.0.remove(&item_id) {
            commands.entity(item_info.entity).despawn();
        }

        // Update player's power-up timer
        if let Some(player_info) = players.0.get_mut(&player_id) {
            match item_type {
                ItemType::SpeedPowerUp => {
                    player_info.speed_power_up_timer = POWER_UP_SPEED_DURATION;
                }
                ItemType::MultiShotPowerUp => {
                    player_info.multi_shot_power_up_timer = POWER_UP_MULTI_SHOT_DURATION;
                }
                ItemType::ReflectPowerUp => {
                    player_info.reflect_power_up_timer = POWER_UP_REFLECT_DURATION;
                }
                ItemType::PhasingPowerUp => {
                    player_info.phasing_power_up_timer = POWER_UP_PHASING_DURATION;
                }
                ItemType::GhostHuntPowerUp => {
                    player_info.ghost_hunt_power_up_timer = POWER_UP_GHOST_HUNT_DURATION;
                }
                ItemType::Cookie => unreachable!(), // Already handled above
            }

            power_up_messages.push(SPlayerStatus {
                id: player_id,
                speed_power_up: ALWAYS_SPEED || player_info.speed_power_up_timer > 0.0,
                multi_shot_power_up: ALWAYS_MULTI_SHOT || player_info.multi_shot_power_up_timer > 0.0,
                reflect_power_up: ALWAYS_REFLECT || player_info.reflect_power_up_timer > 0.0,
                phasing_power_up: ALWAYS_PHASING || player_info.phasing_power_up_timer > 0.0,
                ghost_hunt_power_up: ALWAYS_GHOST_HUNT || player_info.ghost_hunt_power_up_timer > 0.0,
                stunned: player_info.stun_timer > 0.0,
            });
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
            }
        }
    }
}
