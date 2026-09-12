use std::collections::HashSet;

use bevy::prelude::*;
use rand::{rng, seq::SliceRandom};

use crate::{
    items::{ItemInfo, ItemMap, ItemPlacement, ItemSpawner, RandomItems},
    map::MapConfig,
};
use common::{
    map::MapGeometry,
    protocol::{CarrierId, ItemId, ItemMarker, Position},
};

use super::spawn_cells::{
    ItemSpawnCell, choose_item_type, eligible_item_spawn_cells, item_spawn_cell_from_position,
    target_active_random_items,
};

// One entity per map-authored item, spawned once at startup. The entity
// stays at its cell forever — pickup hides it and the countdown re-shows it.
// The position is in the item's carrier frame.
pub fn placed_item_spawn_system(
    mut commands: Commands,
    mut spawner: ResMut<ItemSpawner>,
    mut items: ResMut<ItemMap>,
    map_config: Res<MapConfig>,
) {
    for placed in &map_config.placed_items {
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

    let active_random = items
        .values()
        .filter(|info| matches!(info.placement, ItemPlacement::Random { .. }))
        .count();
    if active_random >= random_items.max_number {
        return;
    }

    // Random items land on the map itself, never on a nested map.
    let eligible_cells = eligible_item_spawn_cells(map_config.root_grid());
    let target_active = target_active_random_items(eligible_cells.len(), random_items.max_number);
    let missing = target_active.saturating_sub(active_random);
    if missing == 0 {
        return;
    }

    // All items on the map claim their cell — including hidden placed
    // ones, so a random item can't land on a gold cell mid-respawn. A
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
    let mut available_cells = eligible_cells
        .into_iter()
        .filter(|cell| !occupied_cells.contains(cell))
        .collect::<Vec<_>>();
    available_cells.shuffle(&mut rng);
    for spawn_cell in available_cells.into_iter().take(missing) {
        let Some(item_type) = choose_item_type(&mut rng, &random_items.pool) else {
            break;
        };
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

#[cfg(test)]
#[path = "tests/spawning.rs"]
mod tests;
