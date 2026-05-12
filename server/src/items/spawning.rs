use std::collections::HashSet;

use bevy::prelude::*;
use rand::{RngExt, rng};

use crate::{
    constants::COOKIE_SPAWNING_ENABLED,
    resources::{ItemInfo, ItemMap, ItemSpawner, MapConfig},
};
use common::{
    map_geometry::MapGeometry,
    protocol::{ItemId, ItemMarker, ItemType, Position},
};

use super::spawn_cells::{
    ItemSpawnCell, choose_item_type, eligible_item_spawn_cells, item_spawn_cell_from_position, power_up_spawn_interval,
    target_active_power_ups,
};

pub fn item_initial_spawn_system(
    mut commands: Commands,
    mut spawner: ResMut<ItemSpawner>,
    mut items: ResMut<ItemMap>,
    query: Query<&ItemId, With<ItemMarker>>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
) {
    if !COOKIE_SPAWNING_ENABLED {
        return;
    }

    let has_cookies = query
        .iter()
        .any(|id| items.get(id).is_some_and(|info| info.item_type == ItemType::Cookie));

    if has_cookies {
        return;
    }

    // Cookies only spawn in cells inside an editor-authored cookie spawn zone.
    // Outside those zones, the floor stays cookie-free.
    let zone_cells: HashSet<(u8, i32, i32)> = map_config
        .cookie_spawn_zones
        .iter()
        .flat_map(|zone| zone.cells().map(move |(c, r)| (zone.level, c, r)))
        .collect();

    for spawn_cell in eligible_item_spawn_cells(&map_config) {
        if !zone_cells.contains(&(spawn_cell.level, spawn_cell.col, spawn_cell.row)) {
            continue;
        }
        let item_id = ItemId(spawner.next_id);
        spawner.next_id += 1;
        let position = spawn_cell.position(&map_geometry);

        let entity = commands.spawn((ItemMarker, item_id, position)).id();

        items.insert(
            item_id,
            ItemInfo {
                entity,
                item_type: ItemType::Cookie,
                spawn_time: 0.0,
            },
        );
    }
}

// Spawn one world key per `KeySpawnZone` on startup. Each zone's first
// eligible (`has_floor && !has_ramp`) cell becomes the key's home; the
// entity stays at that cell forever (respawn just hides/shows it).
pub fn key_initial_spawn_system(
    mut commands: Commands,
    mut spawner: ResMut<ItemSpawner>,
    mut items: ResMut<ItemMap>,
    query: Query<&ItemId, With<ItemMarker>>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
) {
    let has_keys = query.iter().any(|id| {
        items
            .get(id)
            .is_some_and(|info| matches!(info.item_type, ItemType::Key(_)))
    });
    if has_keys {
        return;
    }

    let eligible = eligible_item_spawn_cells(&map_config);

    for (zone_idx, zone) in map_config.key_spawn_zones.iter().enumerate() {
        let zone_cell = eligible.iter().find(|c| {
            c.level == zone.level
                && c.col >= zone.cols[0]
                && c.col < zone.cols[1]
                && c.row >= zone.rows[0]
                && c.row < zone.rows[1]
        });

        let Some(cell) = zone_cell else {
            warn!(
                "key spawn zone {} (kind {:?}) has no eligible cells; skipping",
                zone_idx, zone.kind
            );
            continue;
        };

        let item_id = ItemId(spawner.next_id);
        spawner.next_id += 1;
        let position = cell.position(&map_geometry);
        let entity = commands.spawn((ItemMarker, item_id, position)).id();
        items.insert(
            item_id,
            ItemInfo {
                entity,
                item_type: ItemType::Key(zone.kind),
                spawn_time: 0.0,
            },
        );
    }
}

pub fn item_spawn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut spawner: ResMut<ItemSpawner>,
    mut items: ResMut<ItemMap>,
    positions: Query<&Position, With<ItemMarker>>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
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
            .values()
            .filter(|info| info.item_type != ItemType::Cookie)
            .filter_map(|info| {
                positions
                    .get(info.entity)
                    .ok()
                    .map(|pos| item_spawn_cell_from_position(&map_geometry, pos))
            })
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
        if !available_cells.is_empty()
            && let Some(item_type) = choose_item_type(&mut rng)
        {
            let spawn_cell = available_cells[rng.random_range(0..available_cells.len())];
            let item_id = ItemId(spawner.next_id);
            spawner.next_id += 1;
            let position = spawn_cell.position(&map_geometry);

            let entity = commands.spawn((ItemMarker, item_id, position)).id();

            items.insert(
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
