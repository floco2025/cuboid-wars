use std::collections::HashSet;

use bevy::prelude::*;
use rand::{RngExt, rng};

use crate::items::{ItemInfo, ItemMap, ItemPlacement, ItemSpawner, RandomItems};
use crate::map::MapConfig;
use common::{
    map::MapGeometry,
    protocol::{CarrierId, ItemId, ItemMarker, MapSettings, Position},
};

use super::spawn_cells::{
    ItemSpawnCell, choose_item_type, eligible_item_spawn_cells, item_spawn_cell_from_position,
    random_item_spawn_interval, target_active_random_items,
};

// One entity per map-authored item, spawned once at startup. The entity
// stays at its cell forever — pickup hides it and the countdown re-shows it.
// The position is in the item's carrier frame.
pub fn placed_item_spawn_system(
    mut commands: Commands,
    mut spawner: ResMut<ItemSpawner>,
    mut items: ResMut<ItemMap>,
    map_config: Res<MapConfig>,
    map_settings: Res<MapSettings>,
) {
    for placed in &map_config.placed_items {
        if !map_settings.weapons.allows_item(placed.item_type) {
            continue;
        }
        let item_id = ItemId(spawner.next_id);
        spawner.next_id += 1;
        let position = ItemSpawnCell {
            level: placed.level,
            col: placed.col,
            row: placed.row,
        }
        .position(&map_config.grid(placed.carrier).geometry);

        let entity = commands.spawn((ItemMarker, item_id, position)).id();

        items.insert(
            item_id,
            ItemInfo {
                entity,
                item_type: placed.item_type,
                placement: ItemPlacement::Placed { respawn_countdown: 0.0 },
                carrier: placed.carrier,
            },
        );
    }
}

pub fn random_item_spawn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut spawner: ResMut<ItemSpawner>,
    mut items: ResMut<ItemMap>,
    positions: Query<&Position, With<ItemMarker>>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
    random_items: Res<RandomItems>,
) {
    if random_items.pool.is_empty() {
        return;
    }

    let delta = time.delta_secs();
    spawner.timer += delta;

    // Random items land on the map itself, never on a nested map.
    let eligible_cells = eligible_item_spawn_cells(map_config.root_grid());
    let target_active = target_active_random_items(eligible_cells.len(), random_items.max_number);
    let Some(spawn_interval) = random_item_spawn_interval(random_items.despawn_secs, target_active) else {
        return;
    };

    if spawner.timer >= spawn_interval {
        spawner.timer = 0.0;

        let active_random = items
            .values()
            .filter(|info| matches!(info.placement, ItemPlacement::Random { .. }))
            .count();
        if active_random >= target_active {
            return;
        }

        // All items on the map claim their cell — including hidden placed
        // ones, so a random item can't land on a cookie cell mid-respawn. A
        // carried item's position is in another grid and claims nothing here.
        let occupied_cells: HashSet<ItemSpawnCell> = items
            .values()
            .filter(|info| info.carrier.is_world())
            .filter_map(|info| {
                positions
                    .get(info.entity)
                    .ok()
                    .map(|pos| item_spawn_cell_from_position(&map_geometry, pos))
            })
            .collect();

        let mut rng = rng();
        let available_cells = eligible_cells
            .into_iter()
            .filter(|cell| !occupied_cells.contains(cell))
            .collect::<Vec<_>>();
        if !available_cells.is_empty()
            && let Some(item_type) = choose_item_type(&mut rng, &random_items.pool)
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
                    placement: ItemPlacement::Random {
                        spawned_at: time.elapsed_secs(),
                    },
                    carrier: CarrierId::WORLD,
                },
            );
        }
    }
}
