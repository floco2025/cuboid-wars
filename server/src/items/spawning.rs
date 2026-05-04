use std::collections::HashSet;

use bevy::prelude::*;
use rand::{RngExt, rng};

use crate::{
    constants::COOKIE_SPAWNING_ENABLED,
    resources::{ItemInfo, ItemMap, ItemSpawner, MapConfig},
};
use common::protocol::{ItemId, ItemMarker, ItemType, Position};

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
) {
    if !COOKIE_SPAWNING_ENABLED {
        return;
    }

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
