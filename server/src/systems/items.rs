use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};
use std::collections::HashSet;

use super::network::broadcast_to_all;
use crate::{
    constants::*,
    map::grid_coords_from_position,
    net::ServerToClient,
    resources::{ItemInfo, ItemMap, ItemSpawner, MapConfig, PlayerMap},
};
use common::{
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT, MAP_DEPTH, MAP_WIDTH},
    map::compute_player_level,
    markers::{ItemMarker, PlayerMarker},
    physics::overlap_player_vs_item,
    protocol::{ItemId, ItemType, PlayerId, Position, SCookieCollected, ServerMessage},
};

// ============================================================================
// Helper Functions
// ============================================================================

const ITEM_PICKUP_FLOOR_EPSILON: f32 = 0.1;

fn choose_item_type(rng: &mut ThreadRng) -> ItemType {
    let rand_val = rng.random::<f64>();
    if rand_val < 1.0 / 3.0 {
        ItemType::SpeedPowerUp
    } else if rand_val < 2.0 / 3.0 {
        ItemType::MultiShotPowerUp
    } else {
        ItemType::PhasingPowerUp
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct ItemSpawnCell {
    level: u8,
    col: i32,
    row: i32,
}

impl ItemSpawnCell {
    fn position(self) -> Position {
        Position {
            x: (self.col as f32 + 0.5).mul_add(GRID_CELL_SIZE, -(MAP_WIDTH / 2.0)),
            y: f32::from(self.level) * LEVEL_HEIGHT,
            z: (self.row as f32 + 0.5).mul_add(GRID_CELL_SIZE, -(MAP_DEPTH / 2.0)),
        }
    }
}

fn item_spawn_cell_from_position(pos: &Position) -> ItemSpawnCell {
    let (col, row) = grid_coords_from_position(pos);
    ItemSpawnCell {
        level: compute_player_level(pos.y),
        col,
        row,
    }
}

fn eligible_item_spawn_cells(map_config: &MapConfig) -> Vec<ItemSpawnCell> {
    let mut cells = Vec::new();
    for (level_idx, level_grid) in map_config.levels.iter().enumerate() {
        let level = u8::try_from(level_idx).unwrap_or(u8::MAX);
        for (row, grid_row) in level_grid.cells.rows.iter().enumerate() {
            for (col, cell) in grid_row.iter().enumerate() {
                if cell.has_floor && !cell.has_ramp {
                    cells.push(ItemSpawnCell {
                        level,
                        col: col as i32,
                        row: row as i32,
                    });
                }
            }
        }
    }
    cells
}

fn target_active_power_ups(eligible_cell_count: usize) -> usize {
    if eligible_cell_count == 0 {
        return 0;
    }
    eligible_cell_count
        .div_ceil(ITEM_CELLS_PER_ACTIVE)
        .clamp(ITEM_MIN_ACTIVE, ITEM_MAX_ACTIVE)
        .min(eligible_cell_count)
        .max(1)
}

fn power_up_spawn_interval(eligible_cell_count: usize) -> Option<f32> {
    let target_active = target_active_power_ups(eligible_cell_count);
    (target_active > 0).then_some(ITEM_LIFETIME / target_active as f32)
}

// ============================================================================
// Item Spawn/Despawn Systems
// ============================================================================

// System to spawn cookies on all grid cells at startup
pub fn item_initial_spawn_system(
    mut commands: Commands,
    mut spawner: ResMut<ItemSpawner>,
    mut items: ResMut<ItemMap>,
    query: Query<&ItemId, With<ItemMarker>>,
    map_config: Res<MapConfig>,
) {
    if !COOKIE_SPAWNING_ENABLED {
        return;
    }

    // Only spawn cookies once - check if any cookies exist
    let has_cookies = query
        .iter()
        .any(|id| items.0.get(id).is_some_and(|info| info.item_type == ItemType::Cookie));

    if has_cookies {
        return;
    }

    for spawn_cell in eligible_item_spawn_cells(&map_config) {
        let item_id = ItemId(spawner.next_id);
        spawner.next_id += 1;
        let position = spawn_cell.position();

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

// System to spawn items at regular intervals
pub fn item_spawn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut spawner: ResMut<ItemSpawner>,
    mut items: ResMut<ItemMap>,
    positions: Query<&Position, With<ItemMarker>>,
    map_config: Res<MapConfig>,
) {
    let delta = time.delta_secs();
    spawner.timer += delta;

    let eligible_cells = eligible_item_spawn_cells(&map_config);
    let Some(spawn_interval) = power_up_spawn_interval(eligible_cells.len()) else {
        return;
    };

    if spawner.timer >= spawn_interval {
        spawner.timer = 0.0;

        let occupied_cells: HashSet<ItemSpawnCell> = items
            .0
            .values()
            .filter(|info| info.item_type != ItemType::Cookie)
            .filter_map(|info| positions.get(info.entity).ok().map(item_spawn_cell_from_position))
            .collect();
        let target_active = target_active_power_ups(eligible_cells.len());
        if occupied_cells.len() >= target_active {
            return;
        }

        let mut rng = rng();
        let available_cells = eligible_cells
            .into_iter()
            .filter(|cell| !occupied_cells.contains(cell))
            .collect::<Vec<_>>();
        if !available_cells.is_empty() {
            let spawn_cell = available_cells[rng.random_range(0..available_cells.len())];
            let item_id = ItemId(spawner.next_id);
            spawner.next_id += 1;
            let position = spawn_cell.position();
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
                if let Ok(player_pos) = player_positions.get(player_info.entity) {
                    // Only allow pickup when the player is on the item's floor level.
                    if (player_pos.y - item_pos.y).abs() > ITEM_PICKUP_FLOOR_EPSILON {
                        continue;
                    }

                    if overlap_player_vs_item(player_pos, item_pos, ITEM_COLLECTION_RADIUS) {
                        return Some((*player_id, *item_id, item_info.item_type));
                    }
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
                ItemType::PhasingPowerUp => {
                    player_info.phasing_power_up_timer = POWER_UP_PHASING_DURATION;
                }
                ItemType::Cookie => unreachable!(), // Already handled above
            }

            power_up_messages.push(player_info.status(player_id));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{CellGrid, EdgeGrid, LevelGrid};

    fn map_config(levels: Vec<LevelGrid>) -> MapConfig {
        MapConfig {
            levels,
            player_spawn_fields: Vec::new(),
        }
    }

    fn level_grid(cells: CellGrid) -> LevelGrid {
        LevelGrid {
            cells,
            edges: EdgeGrid::new(1, 1),
        }
    }

    #[test]
    fn item_spawn_cells_include_all_floor_levels_and_skip_ramps() {
        let mut lower = CellGrid::new(1, 1);
        lower.rows[0][0].has_floor = true;
        let mut upper = CellGrid::new(1, 1);
        upper.rows[0][0].has_floor = true;
        upper.rows[0][0].has_ramp = true;
        let config = map_config(vec![level_grid(lower), level_grid(upper)]);

        let cells = eligible_item_spawn_cells(&config);

        assert_eq!(
            cells,
            vec![ItemSpawnCell {
                level: 0,
                col: 0,
                row: 0
            }]
        );
    }

    #[test]
    fn power_up_target_scales_with_eligible_floor_space() {
        assert_eq!(target_active_power_ups(0), 0);
        assert_eq!(target_active_power_ups(1), 1);
        assert_eq!(target_active_power_ups(ITEM_CELLS_PER_ACTIVE * 3), 3);
        assert_eq!(
            target_active_power_ups(ITEM_CELLS_PER_ACTIVE * (ITEM_MAX_ACTIVE + 5)),
            ITEM_MAX_ACTIVE
        );
    }
}
