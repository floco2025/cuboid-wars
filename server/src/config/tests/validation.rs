use super::*;
use crate::config::fixtures;
use crate::{
    map::{ActorSpawnZone, CarrierGrid, CellGrid, EdgeGrid, LevelGrid, PlacedItem},
    test_geometry::geometry,
};
use common::protocol::{CarrierId, QuestId, QuestScope};

fn config_and_map() -> (ServerGameplayConfig, MapConfig) {
    let server = fixtures::server_config();
    let mut map = MapConfig::for_grid(Vec::new(), geometry(2, 2));
    map.actor_spawn_zones.push(ActorSpawnZone {
        switch_inverted: false,
        carrier: CarrierId::WORLD,
        level: 0,
        levels: 1,
        roam_distance: 0.0,
        cols: [0, 1],
        rows: [0, 1],
        kind: "scuttler".into(),
        count: 1,
        respawn_secs: None,
        switch: None,
    });
    (server, map)
}

#[test]
fn immovable_zones_are_not_limited_by_floor_capacity() {
    let server = fixtures::server_config();
    let mut map = MapConfig::for_grid(Vec::new(), geometry(4, 1));
    let mut cells = CellGrid::new(4, 1);
    cells.rows[0][0].has_floor = true;
    cells.rows[0][1].has_floor = true;
    cells.rows[0][1].has_ramp = true;
    cells.rows[0][2].has_floor_slab = true;
    map.grids.push(CarrierGrid::new(
        CarrierId(1),
        geometry(4, 1),
        vec![LevelGrid {
            cells,
            edges: EdgeGrid::new(4, 1),
            barrier_edges: EdgeGrid::new(4, 1),
        }],
    ));
    map.actor_spawn_zones.push(ActorSpawnZone {
        switch_inverted: false,

        carrier: CarrierId(1),
        level: 0,
        levels: 1,
        roam_distance: 0.0,
        cols: [0, 4],
        rows: [0, 1],
        kind: "turret".into(),
        count: 1,
        respawn_secs: None,
        switch: None,
    });
    validate_map_actor_kinds(&server, &map).expect("one turret rejected");
    map.actor_spawn_zones[0].count = 2;
    validate_map_actor_kinds(&server, &map).expect("immovable zone rejected for floor capacity");
    map.actor_spawn_zones[0].kind = "scuttler".into();
    map.actor_spawn_zones[0].count = 100;
    validate_map_actor_kinds(&server, &map).expect("movable actor count limited by cell count");
}

#[test]
fn missing_server_actor_kind_is_rejected() {
    let (mut server, map) = config_and_map();
    server.actors.kinds.remove("scuttler");

    let error = validate_map_actor_kinds(&server, &map).expect_err("missing server actor kind must fail");

    assert!(error.to_string().contains("unknown actor kind"));
}

#[test]
fn gold_quests_require_placed_gold_or_positive_random_weight() {
    let mut map = MapConfig::for_grid(Vec::new(), geometry(1, 1));
    let quests = [Quest {
        id: QuestId("collect_gold".to_owned()),
        kind: QuestKind::Gold,
        scope: QuestScope::Individual,
        requires: None,
        actor_kind: None,
        threshold: 1,
        points: 100,
        title: "Collect gold".to_owned(),
        description: "Find a coin".to_owned(),
        completed_text: "Coin collected".to_owned(),
    }];
    let mut random = RandomItemsConfig {
        weights: [("gold".to_owned(), 0.0), ("speed".to_owned(), 1.0)].into(),
        max_number: 3,
        despawn_secs: 10.0,
    };
    assert!(validate_map_quests(&quests, &map, Some(&random), None).is_err());
    random.weights.insert("gold".to_owned(), 0.5);
    validate_map_quests(&quests, &map, Some(&random), None).expect("random gold quest rejected");
    random.weights.remove("gold");
    assert!(validate_map_quests(&quests, &map, Some(&random), None).is_err());
    map.placed_items.push(PlacedItem {
        carrier: CarrierId::WORLD,
        level: 0,
        col: 0,
        row: 0,
        item_type: ItemType::Gold,
    });
    random.weights.insert("gold".to_owned(), 0.0);
    validate_map_quests(&quests, &map, Some(&random), None).expect("placed gold quest rejected");
}
