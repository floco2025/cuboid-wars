use super::*;
use crate::config::fixtures;
use crate::{
    map::{ActorSpawnZone, CarrierGrid, CellGrid, EdgeGrid, LevelGrid},
    test_geometry::geometry,
};
use common::protocol::CarrierId;

fn config_and_map() -> (ServerGameplayConfig, MapConfig) {
    let server = fixtures::server_config();
    let mut map = MapConfig::for_grid(Vec::new(), geometry(2, 2));
    map.actor_spawn_zones.push(ActorSpawnZone {
        switch_inverted: false,
        carrier: CarrierId::WORLD,
        level: 0,
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
fn immovable_capacity_counts_only_usable_floors_on_the_zones_carrier() {
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
        cols: [0, 4],
        rows: [0, 1],
        kind: "turret".into(),
        count: 1,
        respawn_secs: None,
        switch: None,
    });
    validate_map_actor_kinds(&server, &map).expect("one turret rejected");
    map.actor_spawn_zones[0].count = 2;
    let error = validate_map_actor_kinds(&server, &map).expect_err("overfilled immovable zone accepted");
    assert!(error.to_string().contains("only 1 usable floor cells"), "{error}");
    assert!(error.to_string().contains("carrier 1"), "{error}");
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
