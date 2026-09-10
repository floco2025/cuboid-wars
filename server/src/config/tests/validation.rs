use super::*;
use crate::{
    map::{ActorSpawnZone, CarrierGrid, CellGrid, EdgeGrid, LevelGrid},
    test_geometry::geometry,
};
use common::protocol::CarrierId;

fn config_and_map() -> (ServerGameplayConfig, MapConfig) {
    let server = ServerGameplayConfig::load_default().expect("load server gameplay");
    let settings = &server.maps.get("hotel").expect("hotel settings missing").settings;
    let (barrier_kinds, bridge_kinds, switch_table) = settings.kind_tables().expect("hotel kind tables rejected");
    let map = crate::map::generate_map(
        "hotel",
        server.network.server_hz,
        settings,
        &barrier_kinds,
        &bridge_kinds,
        &switch_table,
    )
    .expect("hotel map failed to generate")
    .config;
    (server, map)
}

#[test]
fn immovable_capacity_counts_only_usable_floors_on_the_zones_carrier() {
    let server = ServerGameplayConfig::load_default().expect("gameplay config rejected");
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
        carrier: CarrierId(1),
        level: 0,
        cols: [0, 4],
        rows: [0, 1],
        kind: "turret".into(),
        count: 1,
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
