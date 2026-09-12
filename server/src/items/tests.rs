use bevy::prelude::*;
use rand::{SeedableRng, rngs::StdRng};

use common::{
    map::MapGeometry,
    protocol::{BarrierKindId, CarrierId, ItemMarker, ItemType},
};

use crate::{
    config::RandomItemsConfig,
    items::{ItemMap, ItemSpawner, RandomItems},
    map::{CellGrid, EdgeGrid, LevelGrid, MapConfig, PlacedItem},
    test_geometry::geometry,
};

use super::{
    spawn_cells::{ItemSpawnCell, choose_item_type, eligible_item_spawn_cells, target_active_random_items},
    spawning::placed_item_spawn_system,
};

fn map_config(levels: Vec<LevelGrid>, geometry: MapGeometry) -> MapConfig {
    MapConfig::for_grid(levels, geometry)
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
    let config = map_config(vec![level_grid(lower), level_grid(upper)], geometry(1, 1));

    let cells = eligible_item_spawn_cells(config.root_grid());

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
    let mut config = map_config(vec![level_grid(cells)], geometry(2, 1));
    config.placed_items = vec![
        PlacedItem {
            carrier: CarrierId::WORLD,
            level: 0,
            col: 0,
            row: 0,
            item_type: ItemType::Gold,
        },
        PlacedItem {
            carrier: CarrierId::WORLD,
            level: 0,
            col: 1,
            row: 0,
            item_type: ItemType::Key(BarrierKindId(0)),
        },
    ];

    let mut world = World::new();
    world.insert_resource(config);
    world.insert_resource(geometry(2, 1));
    world.insert_resource(ItemMap::default());
    world.insert_resource(ItemSpawner::default());
    let mut schedule = Schedule::default();
    schedule.add_systems(placed_item_spawn_system);
    schedule.run(&mut world);

    let items = world.resource::<ItemMap>();
    assert_eq!(items.iter().count(), 2);
    assert!(items.values().all(|info| !info.is_hidden()));
    let spawned_types: Vec<ItemType> = items.values().map(|info| info.item_type).collect();
    assert!(spawned_types.contains(&ItemType::Gold));
    assert!(spawned_types.contains(&ItemType::Key(BarrierKindId(0))));
    let mut marker_query = world.query_filtered::<(), With<ItemMarker>>();
    assert_eq!(marker_query.iter(&world).count(), 2);
}

#[test]
fn placed_item_spawn_system_spawns_all_projectile_and_missile_pickups() {
    let mut cells = CellGrid::new(3, 1);
    for cell in &mut cells.rows[0] {
        cell.has_floor = true;
    }
    let mut config = map_config(vec![level_grid(cells)], geometry(3, 1));
    config.placed_items = vec![
        PlacedItem {
            carrier: CarrierId::WORLD,
            level: 0,
            col: 0,
            row: 0,
            item_type: ItemType::MultiShotPowerUp,
        },
        PlacedItem {
            carrier: CarrierId::WORLD,
            level: 0,
            col: 1,
            row: 0,
            item_type: ItemType::MissilePack,
        },
        PlacedItem {
            carrier: CarrierId::WORLD,
            level: 0,
            col: 2,
            row: 0,
            item_type: ItemType::SingleShotPowerUp,
        },
    ];

    let mut world = World::new();
    world.insert_resource(config);
    world.insert_resource(geometry(3, 1));
    world.insert_resource(ItemMap::default());
    world.insert_resource(ItemSpawner::default());
    let mut schedule = Schedule::default();
    schedule.add_systems(placed_item_spawn_system);
    schedule.run(&mut world);

    let items = world.resource::<ItemMap>();
    assert_eq!(items.iter().count(), 3);
    assert!(items.values().any(|info| info.item_type == ItemType::SingleShotPowerUp));
    assert!(items.values().any(|info| info.item_type == ItemType::MultiShotPowerUp));
    assert!(items.values().any(|info| info.item_type == ItemType::MissilePack));
}

#[test]
fn choose_item_type_returns_none_for_empty_pool() {
    assert_eq!(choose_item_type(&mut rand::rng(), &[]), None);
}

#[test]
fn random_item_selection_follows_relative_weights() {
    let config = RandomItemsConfig {
        weights: [
            ("single_shot".to_owned(), 0.5),
            ("missile_pack".to_owned(), 1.5),
            ("gold".to_owned(), 3.0),
            ("speed".to_owned(), 0.0),
        ]
        .into(),
        max_number: 3,
        despawn_secs: 10.0,
    };
    let random = RandomItems::from_config(Some(&config));
    let mut rng = StdRng::seed_from_u64(42);
    let mut counts = [0; 3];
    for _ in 0..20_000 {
        match choose_item_type(&mut rng, &random.pool).expect("random item selection returned no item") {
            ItemType::SingleShotPowerUp => counts[0] += 1,
            ItemType::MissilePack => counts[1] += 1,
            ItemType::Gold => counts[2] += 1,
            picked => panic!("disabled or omitted item selected: {picked:?}"),
        }
    }
    for (count, expected) in counts.into_iter().zip([0.1, 0.3, 0.6]) {
        let frequency = f64::from(count) / 20_000.0;
        assert!(
            (frequency - expected).abs() < 0.02,
            "{frequency} differs from {expected}"
        );
    }
}

#[test]
fn zero_weight_items_are_excluded_from_selection_and_map_availability() {
    let config = RandomItemsConfig {
        weights: [("single_shot".to_owned(), 0.0), ("gold".to_owned(), 1.0)].into(),
        max_number: 3,
        despawn_secs: 10.0,
    };
    let random = RandomItems::from_config(Some(&config));
    let map = map_config(Vec::new(), geometry(1, 1));
    let available = map.available_items(random.pool.iter().map(|&(item_type, _)| item_type));
    assert_eq!(available.0, vec![ItemType::Gold]);
    let mut rng = StdRng::seed_from_u64(42);
    for _ in 0..50 {
        assert_eq!(choose_item_type(&mut rng, &random.pool), Some(ItemType::Gold));
    }
}

#[cfg(test)]
#[path = "tests/tests_collection_eligibility.rs"]
mod collection_eligibility_tests;
