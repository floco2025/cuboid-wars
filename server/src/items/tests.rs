use bevy::prelude::*;

use common::{
    map_geometry::MapGeometry,
    protocol::{BarrierKindId, ItemMarker, ItemType},
};

use crate::resources::{CellGrid, EdgeGrid, ItemMap, ItemSpawner, LevelGrid, MapConfig, PlacedItem};

use super::{
    spawn_cells::{ItemSpawnCell, choose_item_type, eligible_item_spawn_cells, target_active_random_items},
    spawning::placed_item_spawn_system,
};

fn map_config(levels: Vec<LevelGrid>) -> MapConfig {
    MapConfig {
        levels,
        actor_spawn_zones: Vec::new(),
        player_spawn_zones: Vec::new(),
        placed_items: Vec::new(),
        pressure_plates: Vec::new(),
    }
}

fn level_grid(cells: CellGrid) -> LevelGrid {
    LevelGrid {
        cells,
        edges: EdgeGrid::new(1, 1),
        barrier_edges: EdgeGrid::new(1, 1),
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
fn random_item_target_is_capped_by_eligible_cells() {
    // Empty / undersized maps degrade gracefully; once there's enough room
    // the count is just the configured `max_number`.
    let max_number = 50;
    assert_eq!(target_active_random_items(0, max_number), 0);
    assert_eq!(target_active_random_items(1, max_number), 1);
    assert_eq!(target_active_random_items(max_number - 1, max_number), max_number - 1);
    assert_eq!(target_active_random_items(max_number, max_number), max_number);
    assert_eq!(target_active_random_items(max_number + 1000, max_number), max_number);
}

#[test]
fn placed_item_spawn_system_spawns_every_placed_item_visible() {
    let mut cells = CellGrid::new(2, 1);
    cells.rows[0][0].has_floor = true;
    cells.rows[0][1].has_floor = true;
    let mut config = map_config(vec![level_grid(cells)]);
    config.placed_items = vec![
        PlacedItem {
            level: 0,
            col: 0,
            row: 0,
            item_type: ItemType::Cookie,
        },
        PlacedItem {
            level: 0,
            col: 1,
            row: 0,
            item_type: ItemType::Key(BarrierKindId(0)),
        },
    ];

    let mut world = World::new();
    world.insert_resource(config);
    world.insert_resource(MapGeometry::new(2, 1));
    world.insert_resource(ItemMap::default());
    world.insert_resource(ItemSpawner::default());
    let mut schedule = Schedule::default();
    schedule.add_systems(placed_item_spawn_system);
    schedule.run(&mut world);

    let items = world.resource::<ItemMap>();
    assert_eq!(items.iter().count(), 2);
    assert!(items.values().all(|info| !info.is_hidden()));
    let spawned_types: Vec<ItemType> = items.values().map(|info| info.item_type).collect();
    assert!(spawned_types.contains(&ItemType::Cookie));
    assert!(spawned_types.contains(&ItemType::Key(BarrierKindId(0))));
    let mut marker_query = world.query_filtered::<(), With<ItemMarker>>();
    assert_eq!(marker_query.iter(&world).count(), 2);
}

#[test]
fn choose_item_type_returns_none_for_empty_pool() {
    assert_eq!(choose_item_type(&mut rand::rng(), &[]), None);
}

#[test]
fn choose_item_type_only_picks_pool_members() {
    let pool = [ItemType::SpeedPowerUp, ItemType::Cookie];
    let mut rng = rand::rng();
    for _ in 0..50 {
        let picked = choose_item_type(&mut rng, &pool).expect("non-empty pool must yield an item type");
        assert!(pool.contains(&picked));
    }
}
