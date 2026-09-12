use std::time::Duration;

use super::*;
use crate::{
    config::RandomItemsConfig,
    items::random_item_despawn_system,
    map::{CellGrid, EdgeGrid, LevelGrid, PlacedItem},
    test_geometry::geometry,
};
use common::protocol::ItemType;

fn spawn_world(columns: i32, max_number: usize) -> (World, Schedule) {
    let mut cells = CellGrid::new(columns, 1);
    for cell in &mut cells.rows[0] {
        cell.has_floor = true;
    }
    let geometry = geometry(columns, 1);
    let map = MapConfig::for_grid(
        vec![LevelGrid {
            cells,
            edges: EdgeGrid::new(columns, 1),
            barrier_edges: EdgeGrid::new(columns, 1),
        }],
        geometry,
    );
    let config = RandomItemsConfig {
        weights: [("gold".to_owned(), 1.0)].into(),
        max_number,
        despawn_secs: 10.0,
    };
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.insert_resource(map);
    world.insert_resource(geometry);
    world.insert_resource(ItemMap::default());
    world.insert_resource(ItemSpawner::default());
    world.insert_resource(RandomItems::from_config(Some(&config)));
    let mut schedule = Schedule::default();
    schedule.add_systems((random_item_despawn_system, random_item_spawn_system).chain_ignore_deferred());
    (world, schedule)
}

fn random_ids(world: &World) -> HashSet<ItemId> {
    world
        .resource::<ItemMap>()
        .iter()
        .filter(|(_, item)| matches!(item.placement, ItemPlacement::Random { .. }))
        .map(|(&id, _)| id)
        .collect()
}

fn assert_distinct_cells(world: &mut World, count: usize) {
    let geometry = *world.resource::<MapGeometry>();
    let mut query = world.query_filtered::<&Position, With<ItemMarker>>();
    let cells: HashSet<_> = query
        .iter(world)
        .map(|position| item_spawn_cell_from_position(&geometry, position))
        .collect();
    assert_eq!(query.iter(world).count(), count);
    assert_eq!(cells.len(), count);
}

#[test]
fn first_tick_fills_target_and_expiry_replaces_every_item_in_one_tick() {
    let (mut world, mut schedule) = spawn_world(4, 3);
    schedule.run(&mut world);
    let initial = random_ids(&world);
    assert_eq!(initial.len(), 3);
    assert_distinct_cells(&mut world, 3);

    schedule.run(&mut world);
    assert_eq!(random_ids(&world), initial);

    world.resource_mut::<Time>().advance_by(Duration::from_secs(10));
    schedule.run(&mut world);
    let replacement = random_ids(&world);
    assert_eq!(replacement.len(), 3);
    assert!(replacement.is_disjoint(&initial));
    assert_distinct_cells(&mut world, 3);
}

#[test]
fn refill_reserves_hidden_placed_cells_and_uses_each_free_cell_once() {
    let (mut world, mut schedule) = spawn_world(4, 10);
    world.resource_mut::<MapConfig>().placed_items.push(PlacedItem {
        carrier: CarrierId::WORLD,
        level: 0,
        col: 0,
        row: 0,
        item_type: ItemType::Gold,
    });
    let mut startup = Schedule::default();
    startup.add_systems(placed_item_spawn_system);
    startup.run(&mut world);
    let placed_id = *world
        .resource::<ItemMap>()
        .iter()
        .next()
        .expect("placed item missing")
        .0;
    world
        .resource_mut::<ItemMap>()
        .get_mut(&placed_id)
        .expect("placed item missing")
        .placement = ItemPlacement::Placed {
        respawn_countdown: 30.0,
    };

    schedule.run(&mut world);
    assert_eq!(random_ids(&world).len(), 3);
    assert_distinct_cells(&mut world, 4);

    let removed: Vec<_> = random_ids(&world).into_iter().take(2).collect();
    for id in &removed {
        let item = world.resource_mut::<ItemMap>().remove(id).expect("random item missing");
        world.despawn(item.entity);
    }
    schedule.run(&mut world);
    assert_eq!(random_ids(&world).len(), 3);
    assert!(removed.iter().all(|id| world.resource::<ItemMap>().get(id).is_none()));
    assert!(
        world
            .resource::<ItemMap>()
            .get(&placed_id)
            .expect("placed item missing")
            .is_hidden()
    );
    assert_distinct_cells(&mut world, 4);
}

#[test]
fn refill_skips_maps_without_eligible_cells_and_disabled_random_spawning() {
    let (mut world, mut schedule) = spawn_world(1, 3);
    world.resource_mut::<MapConfig>().grids[0].levels[0].cells.rows[0][0].has_floor = false;
    schedule.run(&mut world);
    assert!(random_ids(&world).is_empty());

    world.resource_mut::<MapConfig>().grids[0].levels[0].cells.rows[0][0].has_floor = true;
    world.insert_resource(RandomItems::from_config(None));
    schedule.run(&mut world);
    assert!(random_ids(&world).is_empty());
}
