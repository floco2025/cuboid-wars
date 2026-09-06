use super::{
    compile_map,
    load::LoadedMaps,
    schema::{
        ActorSpawnZoneDef, BarrierDef, CellDef, FloorDef, ItemDef, LadderDef, LevelDef, LightBridgeDef, MapDef,
        MotionDef, NestedMapDef, PlayerSpawnZoneDef, PressurePlateDef, PressurePlatePurposeDef, RampDef, WallDef,
        WallSide,
    },
    validation::validate_map,
};
use crate::{
    actors::navigation::NavGraph,
    map::MapConfig,
    test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS, sizes},
};
use bevy::math::Vec3;
use common::{
    config::PortalShotSettings,
    map::Carriers,
    physics::{CollisionWorld, compute_portal_placement},
    protocol::{BarrierKindTable, BridgeKindId, BridgeKindTable, CarrierId, FaceMaterials, Position},
};

fn empty_kind_table() -> BarrierKindTable {
    BarrierKindTable::default()
}

fn no_bridges() -> BridgeKindTable {
    BridgeKindTable::default()
}

fn skyway_bridge_table() -> BridgeKindTable {
    BridgeKindTable::from_ids(vec!["skyway".into()]).expect("one-kind bridge table rejected")
}

fn red_only_kind_table() -> BarrierKindTable {
    BarrierKindTable::from_ids(vec!["red".into()]).expect("known-good")
}

fn three_kind_table() -> BarrierKindTable {
    BarrierKindTable::from_ids(vec!["red".into(), "blue".into(), "green".into()]).expect("known-good")
}

fn floor_def(col: i32, row: i32) -> FloorDef {
    FloorDef {
        col,
        row,
        materials: FaceMaterials::uniform("test"),
    }
}

fn cell_def(col: i32, row: i32) -> CellDef {
    CellDef { col, row }
}

fn bridge_def(col: i32, row: i32) -> LightBridgeDef {
    LightBridgeDef {
        col,
        row,
        kind: "skyway".into(),
    }
}

// One floor at (0, 0) plus the bridge cells named, on a 4x4 grid.
fn map_with_bridges(cells: &[[i32; 2]]) -> MapDef {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].light_bridges = cells.iter().map(|[c, r]| bridge_def(*c, *r)).collect();
    map_def
}

fn level(floors: Vec<[i32; 2]>) -> LevelDef {
    level_with_inaccessible(floors, Vec::new())
}

fn level_with_inaccessible(floors: Vec<[i32; 2]>, inaccessible_floors: Vec<[i32; 2]>) -> LevelDef {
    LevelDef {
        name: None,
        floors: floors.into_iter().map(|[c, r]| floor_def(c, r)).collect(),
        inaccessible_floors: inaccessible_floors.into_iter().map(|[c, r]| floor_def(c, r)).collect(),
        grass: Vec::new(),
        walls: Vec::new(),
        barriers: Vec::new(),
        light_bridges: Vec::new(),
        lights: Vec::new(),
    }
}

fn actor_zone(level: u32, col: i32, row: i32) -> ActorSpawnZoneDef {
    ActorSpawnZoneDef {
        level,
        cols: [col, col + 1],
        rows: [row, row + 1],
        kind: "actor".into(),
        count: 1,
    }
}

fn player_zone(level: u32, col: i32, row: i32) -> PlayerSpawnZoneDef {
    PlayerSpawnZoneDef {
        level,
        cols: [col, col + 1],
        rows: [row, row + 1],
    }
}

fn ramp(low: [i32; 2], high: [i32; 2], lower_level: u32) -> RampDef {
    RampDef {
        low,
        high,
        lower_level,
        materials: FaceMaterials::uniform("test"),
    }
}

fn map_with_zones(
    grid: i32,
    levels: Vec<LevelDef>,
    actor_spawn_zones: Vec<ActorSpawnZoneDef>,
    player_spawn_zones: Vec<PlayerSpawnZoneDef>,
    ramps: Vec<RampDef>,
) -> MapDef {
    MapDef {
        grid_cols: grid,
        grid_rows: grid,
        actor_spawn_zones,
        player_spawn_zones,
        items: Vec::new(),
        pressure_plates: Vec::new(),
        levels,
        ramps,
        ladders: Vec::new(),
        nested_maps: Vec::new(),
    }
}

fn motion(level: u32, from: [i32; 2], to: [i32; 2], to_level: u32) -> MotionDef {
    MotionDef {
        level,
        from,
        to,
        to_level: Some(to_level),
        travel_secs: 2.0,
        pause_secs: 0.5,
        phase_secs: 0.0,
        from_nudge: [0.0; 3],
        to_nudge: [0.0; 3],
    }
}

fn nested(map: &str, level: u32, from: [i32; 2], to: [i32; 2], to_level: u32) -> NestedMapDef {
    NestedMapDef {
        map: map.into(),
        motion: motion(level, from, to, to_level),
    }
}

fn no_nested() -> LoadedMaps {
    LoadedMaps::default()
}

fn ladder(lower_level: u32, col: i32, row: i32, side: WallSide, levels: u32) -> LadderDef {
    LadderDef {
        lower_level,
        col,
        row,
        side,
        levels,
    }
}

#[test]
fn validation_accepts_actor_zone_without_floor() {
    // Empty cells (no floor at all) are allowed: kinds like flying actors
    // don't need a floor underfoot. Forbidden cells are obstructions.
    let map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[1, 0]])],
        vec![actor_zone(1, 0, 0)],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("a zone over an empty cell should load");
}

#[test]
fn validation_accepts_actor_zone_on_higher_level_floor() {
    let map_def = map_with_zones(
        4,
        vec![level(vec![[1, 0]]), level(vec![[0, 0]])],
        vec![actor_zone(1, 0, 0)],
        vec![player_zone(0, 1, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("zones should be allowed on any level floor");
}

#[test]
fn validation_accepts_actor_zone_overlapping_inaccessible_floor() {
    // Spawn zones may freely cover any cell, including inaccessible-floor slabs.
    // The runtime spawn picker filters non-spawnable cells out at pick time
    // (see `Cell::is_spawnable`), so authoring a zone that brushes one is fine.
    let map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[1, 0]])],
        vec![actor_zone(0, 1, 0)],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("actor zone overlapping inaccessible floor should load");
}

#[test]
fn validation_accepts_player_zone_overlapping_inaccessible_floor() {
    let map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[1, 0]])],
        Vec::new(),
        vec![player_zone(0, 1, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("player zone overlapping inaccessible floor should load");
}

#[test]
fn inaccessible_floor_emits_physical_slab_but_not_regular_floor() {
    let map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[2, 0]])],
        vec![actor_zone(0, 0, 0)],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    let (layout, config) =
        compile_map(&map_def, sizes(), &no_nested(), &empty_kind_table(), &no_bridges()).expect("compile");
    let geometry = config.root_grid().geometry;
    let inaccessible_cell = config.root_grid().levels[0].cells.rows[0][2];
    assert!(!inaccessible_cell.has_floor);
    assert!(inaccessible_cell.has_floor_slab);

    let x = geometry.cell_center_x(2);
    let z = geometry.cell_center_z(0);
    assert!(layout.floors.iter().any(|floor| {
        let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
        min_x <= x && x <= max_x && min_z <= z && z <= max_z
    }));
}

#[test]
fn validation_accepts_actor_zone_overlapping_ramp_footprint() {
    // Ramp footprints are not spawnable, but a zone is free to brush one;
    // the spawn picker skips ramp cells (see `Cell::is_spawnable`).
    let map_def = map_with_zones(
        4,
        vec![level(vec![[3, 3]]), level(vec![[0, 0]]), level(vec![[3, 3]])],
        vec![actor_zone(1, 0, 0)],
        vec![player_zone(0, 3, 3)],
        vec![ramp([0, 0], [1, 2], 1)],
    );

    validate_map(&map_def).expect("actor zone overlapping ramp footprint should load");
}

#[test]
fn validation_accepts_player_zone_overlapping_ramp_footprint() {
    let map_def = map_with_zones(
        4,
        vec![level(vec![[3, 3]]), level(vec![[0, 0]]), level(vec![[3, 3]])],
        vec![],
        vec![player_zone(1, 0, 0)],
        vec![ramp([0, 0], [1, 2], 1)],
    );

    validate_map(&map_def).expect("player zone overlapping ramp footprint should load");
}

#[test]
fn validation_rejects_actor_zone_with_empty_kind() {
    let map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        vec![ActorSpawnZoneDef {
            level: 0,
            cols: [0, 1],
            rows: [0, 1],
            kind: String::new(),
            count: 1,
        }],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    let err = validate_map(&map_def).expect_err("must reject empty `kind`");
    assert!(err.to_string().contains("empty `kind`"));
}

#[test]
fn validation_accepts_unknown_kind_strings() {
    // The map loader knows nothing about specific kinds; whether a kind is
    // useful is the spawn picker's call. So unfamiliar strings must load fine.
    let map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        vec![ActorSpawnZoneDef {
            level: 0,
            cols: [0, 1],
            rows: [0, 1],
            kind: "boss".into(),
            count: 1,
        }],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("any non-empty kind string should load");
}

#[test]
fn validation_accepts_empty_actor_spawn_zones() {
    // A map with no enemies is valid.
    let map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );

    validate_map(&map_def).expect("empty actor zones should be allowed");
}

#[test]
fn validation_accepts_barrier_on_empty_edge() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        kind: "red".into(),
    });
    validate_map(&map_def).expect("barrier on an empty grid edge should load");
}

#[test]
fn validation_rejects_barrier_overlapping_wall() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].walls.push(WallDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        materials: FaceMaterials::uniform("test"),
    });
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 1,
        r0: 0,
        c1: 0,
        r1: 0,
        kind: "blue".into(),
    });
    let err = validate_map(&map_def).expect_err("barrier on a wall edge must be rejected");
    let msg = err.to_string();
    assert!(msg.contains("overlaps a wall"), "got: {msg}");
}

#[test]
fn compile_resolves_known_barrier_kind() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        kind: "red".into(),
    });
    let (layout, _) =
        compile_map(&map_def, sizes(), &no_nested(), &red_only_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.barriers.len(), 1);
    assert_eq!(layout.barriers[0].kind, common::protocol::BarrierKindId(0));
}

#[test]
fn stacked_barriers_compile_into_one_record_when_no_floor_splits_them() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[2, 2]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    for level in &mut map_def.levels {
        level.barriers.push(BarrierDef {
            c0: 0,
            r0: 0,
            c1: 1,
            r1: 0,
            kind: "red".into(),
        });
    }
    let (layout, _) =
        compile_map(&map_def, sizes(), &no_nested(), &red_only_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.barriers.len(), 1);
    assert_eq!(layout.barriers[0].level, 0);
    assert_eq!(layout.barriers[0].levels, 2);
    assert_eq!(layout.barriers[0].height, LEVEL_HEIGHT + WALL_HEIGHT);
}

#[test]
fn a_floor_beside_the_upper_barrier_keeps_the_storeys_apart() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    for level in &mut map_def.levels {
        level.barriers.push(BarrierDef {
            c0: 0,
            r0: 0,
            c1: 1,
            r1: 0,
            kind: "red".into(),
        });
    }
    let (layout, _) =
        compile_map(&map_def, sizes(), &no_nested(), &red_only_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.barriers.len(), 2);
    assert!(layout.barriers.iter().all(|barrier| barrier.levels == 1));
}

#[test]
fn pressure_plate_barrier_is_open_for_pathfinding() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    // Vertical edge between cols 0 and 1 → `vertical[0][1]`; kind "red" has a plate.
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 1,
        r0: 0,
        c1: 1,
        r1: 1,
        kind: "red".into(),
    });
    // Vertical edge between cols 1 and 2 → `vertical[0][2]`; kind "blue" has none.
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 2,
        r0: 0,
        c1: 2,
        r1: 1,
        kind: "blue".into(),
    });
    map_def.pressure_plates.push(PressurePlateDef {
        level: 0,
        col: 0,
        row: 0,
        purpose: PressurePlatePurposeDef::Barrier { kind: "red".into() },
    });

    let (_, config) =
        compile_map(&map_def, sizes(), &no_nested(), &three_kind_table(), &no_bridges()).expect("compile");
    let barrier_edges = &config.root_grid().levels[0].barrier_edges;
    assert!(
        !barrier_edges.vertical[0][1],
        "pressure-plate (red) barrier must be treated as open for nav"
    );
    assert!(barrier_edges.vertical[0][2], "non-plate (blue) barrier must block nav");
}

#[test]
fn compiled_wall_trim_blocks_portal_shots_through_the_storey_seam() {
    let mut map = map_with_zones(
        3,
        vec![level(vec![[0, 0]]), level(Vec::new())],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    for level in &mut map.levels {
        for col in [1, 3] {
            level.walls.push(WallDef {
                c0: col,
                r0: 0,
                c1: col,
                r1: 1,
                materials: FaceMaterials::uniform("test"),
            });
        }
    }
    let (layout, config) = compile_map(&map, sizes(), &no_nested(), &empty_kind_table(), &no_bridges())
        .expect("stacked wall map failed to compile");
    let world = CollisionWorld::from_map_layout(&layout, &empty_kind_table());
    let geometry = config.root_grid().geometry;
    let seam_y = geometry.level_y(1) - geometry.floor_thickness() / 2.0;
    let placement = compute_portal_placement(
        Vec3::new(geometry.cell_center_x(0), seam_y, geometry.cell_center_z(0)),
        Vec3::X,
        0.0,
        geometry.width(),
        &world,
        &layout,
        &Carriers::default(),
        PortalShotSettings::default(),
        &[],
    )
    .expect("stacked wall seam has no fitting portal surface");
    let front_face = geometry.cell_to_world_x(1) - geometry.wall_half_thickness();
    assert!((placement.pos.x - front_face).abs() < 1e-4);
    assert!(placement.normal.abs_diff_eq(Vec3::NEG_X, 1e-4));
}

fn barrier_corridor() -> MapDef {
    let mut map = map_with_zones(
        3,
        vec![level(vec![[0, 0], [1, 0], [2, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    for (col, kind) in [(1, "red"), (2, "blue")] {
        map.levels[0].barriers.push(BarrierDef {
            c0: col,
            r0: 0,
            c1: col,
            r1: 1,
            kind: kind.into(),
        });
    }
    map
}

fn red_barrier_plate() -> PressurePlateDef {
    PressurePlateDef {
        level: 0,
        col: 0,
        row: 0,
        purpose: PressurePlatePurposeDef::Barrier { kind: "red".into() },
    }
}

fn assert_only_plate_barrier_allows_a_route(config: &MapConfig, carrier: CarrierId) {
    let grid = config.grid(carrier);
    let nav = NavGraph::new(grid);
    let position = |col| Position {
        x: grid.geometry.cell_center_x(col),
        y: grid.geometry.level_y(0),
        z: grid.geometry.cell_center_z(0),
    };
    assert!(
        nav.engagement_route(&position(0), &position(1), 0.2, 0.2).is_some(),
        "the red barrier's controlling plate must allow an actor route"
    );
    assert!(
        nav.engagement_route(&position(1), &position(2), 0.2, 0.2).is_none(),
        "the blue barrier has no controlling plate and must block actor routes"
    );
}

#[test]
fn a_nested_plate_allows_actor_routes_through_parent_barriers() {
    let mut root = barrier_corridor();
    root.nested_maps.push(nested("switch", 0, [0, 1], [1, 1], 0));
    let mut switch = host(Vec::new());
    switch.pressure_plates.push(red_barrier_plate());
    let (_, config) = compile_map(
        &root,
        sizes(),
        &tree(vec![("switch", switch)]),
        &three_kind_table(),
        &no_bridges(),
    )
    .expect("nested plate map failed to compile");

    assert_only_plate_barrier_allows_a_route(&config, CarrierId::WORLD);
}

#[test]
fn a_parent_plate_allows_actor_routes_through_nested_barriers() {
    let mut root = host(vec![nested("corridor", 0, [0, 1], [1, 1], 0)]);
    root.pressure_plates.push(red_barrier_plate());
    let (_, config) = compile_map(
        &root,
        sizes(),
        &tree(vec![("corridor", barrier_corridor())]),
        &three_kind_table(),
        &no_bridges(),
    )
    .expect("nested barrier map failed to compile");

    assert_only_plate_barrier_allows_a_route(&config, CarrierId(1));
}

#[test]
fn a_deeply_nested_plate_allows_actor_routes_through_a_siblings_barriers() {
    let root = host(vec![
        nested("corridor", 0, [0, 1], [1, 1], 0),
        nested("middle", 0, [0, 3], [1, 3], 0),
    ]);
    let middle = host(vec![nested("switch", 0, [0, 1], [1, 1], 0)]);
    let mut switch = host(Vec::new());
    switch.pressure_plates.push(red_barrier_plate());
    let (_, config) = compile_map(
        &root,
        sizes(),
        &tree(vec![
            ("corridor", barrier_corridor()),
            ("middle", middle),
            ("switch", switch),
        ]),
        &three_kind_table(),
        &no_bridges(),
    )
    .expect("deeply nested plate map failed to compile");

    assert_only_plate_barrier_allows_a_route(&config, CarrierId(1));
}

#[test]
fn firework_plate_does_not_open_any_barrier_kind() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 1,
        r0: 0,
        c1: 1,
        r1: 1,
        kind: "red".into(),
    });
    map_def.pressure_plates.push(PressurePlateDef {
        level: 0,
        col: 0,
        row: 0,
        purpose: PressurePlatePurposeDef::Firework,
    });

    let (layout, config) =
        compile_map(&map_def, sizes(), &no_nested(), &three_kind_table(), &no_bridges()).expect("compile");
    assert!(
        config.root_grid().levels[0].barrier_edges.vertical[0][1],
        "a firework plate opens no barrier kind for nav"
    );
    assert_eq!(
        config.pressure_plates[0].purpose,
        common::protocol::PlatePurpose::Firework
    );
    assert_eq!(
        layout.pressure_plates[0].purpose,
        common::protocol::PlatePurpose::Firework
    );
}

#[test]
fn plate_defs_parse_every_purpose() {
    let json = r#"[
        {"level": 1, "col": 2, "row": 3, "type": "barrier", "kind": "red"},
        {"level": 0, "col": 4, "row": 5, "type": "firework"},
        {"level": 2, "col": 6, "row": 7, "type": "bridge", "kind": "skyway"}
    ]"#;
    let defs: Vec<PressurePlateDef> = serde_json::from_str(json).expect("plate defs parse");
    assert_eq!(defs[0].purpose, PressurePlatePurposeDef::Barrier { kind: "red".into() });
    assert_eq!(defs[1].purpose, PressurePlatePurposeDef::Firework);
    assert_eq!((defs[1].level, defs[1].col, defs[1].row), (0, 4, 5));
    assert_eq!(
        defs[2].purpose,
        PressurePlatePurposeDef::Bridge { kind: "skyway".into() }
    );
}

#[test]
fn compile_merges_light_bridge_cells_into_one_rectangle() {
    let mut map_def = map_with_bridges(&[[1, 0], [2, 0], [1, 1], [2, 1]]);
    map_def.levels[0].floors = vec![floor_def(0, 3)];
    map_def.player_spawn_zones = vec![player_zone(0, 0, 3)];

    let (layout, config) = compile_map(
        &map_def,
        sizes(),
        &no_nested(),
        &empty_kind_table(),
        &skyway_bridge_table(),
    )
    .expect("compile");
    let geometry = config.root_grid().geometry;

    assert_eq!(layout.light_bridges.len(), 1, "a 2x2 block is one collider");
    let bridge = layout.light_bridges[0];
    assert_eq!(bridge.kind, common::protocol::BridgeKindId(0));
    assert_eq!(bridge.level, 0);
    assert!((bridge.y - 0.0).abs() < 1e-4, "level 0 stands at y = 0");
    let (min_x, max_x, min_z, max_z) = bridge.bounds_xz();
    let pad = geometry.wall_half_thickness();
    assert!((min_x - (geometry.cell_to_world_x(1) - pad)).abs() < 1e-4);
    assert!((max_x - (geometry.cell_to_world_x(3) + pad)).abs() < 1e-4);
    assert!((min_z - (geometry.cell_to_world_z(0) - pad)).abs() < 1e-4);
    assert!((max_z - (geometry.cell_to_world_z(2) + pad)).abs() < 1e-4);
}

#[test]
fn portal_shots_cannot_leak_through_compiled_bridge_landing_seams_or_outer_edges() {
    let mut map_def = map_with_bridges(&[[1, 0], [2, 0], [1, 1], [2, 1]]);
    map_def.levels.insert(
        0,
        level((0..4).flat_map(|row| (0..4).map(move |col| [col, row])).collect()),
    );
    let (layout, config) = compile_map(
        &map_def,
        sizes(),
        &no_nested(),
        &empty_kind_table(),
        &skyway_bridge_table(),
    )
    .expect("bridge landing map failed to compile");
    let geometry = config.root_grid().geometry;
    let mut world = CollisionWorld::from_map_layout(&layout, &empty_kind_table());
    let settings = PortalShotSettings {
        barriers_block: true,
        light_bridges_block: true,
    };
    let pad = geometry.wall_half_thickness();
    let floor_edge = geometry.cell_to_world_x(1) + pad;
    let bridge_edge = geometry.cell_to_world_x(3) + pad;
    let zs = [
        geometry.cell_to_world_z(0) - pad + 1e-3,
        geometry.cell_to_world_z(0) + 0.5,
        geometry.cell_to_world_z(1) + pad - 1e-3,
    ];
    world.set_powered_bridges(&[BridgeKindId(0)]);
    for z in zs {
        for x in [floor_edge - 1e-3, floor_edge, floor_edge + 1e-3, bridge_edge - 1e-3] {
            let origin = Vec3::new(x, LEVEL_HEIGHT + 2.0, z);
            if let Some(hit) = world.portal_surface_along_ray(origin, Vec3::NEG_Y, 20.0, settings, &[]) {
                assert!(
                    (hit.point.y - LEVEL_HEIGHT).abs() < 1e-4,
                    "shot leaked to the lower floor at ({x}, {z})"
                );
            }
        }
    }
    world.set_powered_bridges(&[]);
    let hit = world
        .portal_surface_along_ray(
            Vec3::new(bridge_edge - 1e-3, LEVEL_HEIGHT + 2.0, zs[0]),
            Vec3::NEG_Y,
            20.0,
            settings,
            &[],
        )
        .expect("unpowered bridge blocked the lower floor");
    assert!(hit.point.y.abs() < 1e-4);
}

#[test]
fn compile_rejects_unknown_bridge_kind() {
    let mut map_def = map_with_bridges(&[[1, 0]]);
    map_def.levels[0].light_bridges[0].kind = "magenta".into();

    let err = compile_map(
        &map_def,
        sizes(),
        &no_nested(),
        &empty_kind_table(),
        &skyway_bridge_table(),
    )
    .expect_err("unknown bridge kind must fail");
    let chain: String = err.chain().map(|e| e.to_string()).collect::<Vec<_>>().join(" | ");
    assert!(chain.contains("unknown bridge kind"), "got: {chain}");
    assert!(chain.contains("light_bridges[0]"), "got: {chain}");
}

#[test]
fn validate_rejects_a_light_bridge_on_a_floor() {
    let map_def = map_with_bridges(&[[0, 0]]);

    let err = validate_map(&map_def).expect_err("a bridge on a floor must fail");
    assert!(err.to_string().contains("sits on a floor"), "got: {err}");
}

#[test]
fn validate_rejects_a_light_bridge_on_a_ramp() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        vec![ramp([1, 0], [3, 1], 0)],
    );
    map_def.levels[0].light_bridges.push(bridge_def(1, 0));

    let err = validate_map(&map_def).expect_err("a bridge on a ramp must fail");
    assert!(err.to_string().contains("sits on a ramp"), "got: {err}");
}

#[test]
fn validate_rejects_a_plate_on_a_light_bridge() {
    let mut map_def = map_with_bridges(&[[1, 0]]);
    map_def.pressure_plates.push(PressurePlateDef {
        level: 0,
        col: 1,
        row: 0,
        purpose: PressurePlatePurposeDef::Firework,
    });

    let err = validate_map(&map_def).expect_err("a plate on a bridge must fail");
    assert!(err.to_string().contains("sits on a light bridge"), "got: {err}");
}

#[test]
fn validate_rejects_duplicate_light_bridge_cells() {
    let map_def = map_with_bridges(&[[1, 0], [1, 0]]);

    let err = validate_map(&map_def).expect_err("duplicate bridge cells must fail");
    assert!(err.to_string().contains("duplicate light_bridge"), "got: {err}");
}

#[test]
fn compile_rejects_unknown_barrier_kind() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        kind: "magenta".into(),
    });
    let err = compile_map(&map_def, sizes(), &no_nested(), &red_only_kind_table(), &no_bridges())
        .expect_err("unknown kind must fail");
    let chain: String = err.chain().map(|e| e.to_string()).collect::<Vec<_>>().join(" | ");
    assert!(
        chain.to_lowercase().contains("magenta") || chain.to_lowercase().contains("unknown barrier kind"),
        "expected 'magenta' or 'unknown barrier kind' somewhere in chain; got: {chain}"
    );
}

#[test]
fn compile_resolves_three_distinct_kinds() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        kind: "red".into(),
    });
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 1,
        r0: 0,
        c1: 2,
        r1: 0,
        kind: "blue".into(),
    });
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 2,
        r0: 0,
        c1: 3,
        r1: 0,
        kind: "green".into(),
    });
    let (layout, _) =
        compile_map(&map_def, sizes(), &no_nested(), &three_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.barriers.len(), 3);
    let kinds: Vec<u16> = layout.barriers.iter().map(|b| b.kind.0).collect();
    // The merger sorts by (level, kind, axis-coords), so kind ascending.
    assert_eq!(kinds, vec![0, 1, 2]);
}

#[test]
fn compile_drops_grass_without_floor() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].grass.push(cell_def(0, 0));
    map_def.levels[0].grass.push(cell_def(2, 2));
    let (layout, _) =
        compile_map(&map_def, sizes(), &no_nested(), &empty_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.grass.len(), 1);
    assert_eq!(layout.grass[0].level, 0);
}

#[test]
fn grass_compiles_to_cell_center_and_floor_top() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[1, 2]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[1].grass.push(cell_def(1, 2));
    let (layout, config) =
        compile_map(&map_def, sizes(), &no_nested(), &empty_kind_table(), &no_bridges()).expect("compile");
    let geometry = config.root_grid().geometry;
    assert_eq!(layout.grass.len(), 1);
    let cell = layout.grass[0];
    assert_eq!(cell.level, 1);
    let expected_x = geometry.cell_center_x(1);
    let expected_z = geometry.cell_center_z(2);
    assert!((cell.x - expected_x).abs() < 1e-5);
    assert!((cell.z - expected_z).abs() < 1e-5);
    assert!((cell.y - LEVEL_HEIGHT).abs() < 1e-5);
}

#[test]
fn grass_allowed_on_inaccessible_floor() {
    let mut map_def = map_with_zones(
        4,
        vec![level_with_inaccessible(vec![[0, 0]], vec![[1, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].grass.push(cell_def(1, 0));
    validate_map(&map_def).expect("grass on an inaccessible floor should load");
    let (layout, _) =
        compile_map(&map_def, sizes(), &no_nested(), &empty_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.grass.len(), 1);
}

#[test]
fn validation_rejects_grass_out_of_bounds() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].grass.push(cell_def(4, 0));
    let err = validate_map(&map_def).expect_err("out-of-bounds grass must be rejected");
    assert!(err.to_string().contains("grass"));
}

fn item_def(level: u32, col: i32, row: i32, item_type: &str, kind: Option<&str>) -> ItemDef {
    ItemDef {
        level,
        col,
        row,
        item_type: item_type.to_owned(),
        kind: kind.map(str::to_owned),
    }
}

#[test]
fn validation_rejects_item_outside_grid() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 4, 0, "cookie", None));
    let err = validate_map(&map_def).expect_err("out-of-bounds item must be rejected");
    assert!(err.to_string().contains("col"));
}

#[test]
fn validation_rejects_key_item_without_kind() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 0, 0, "key", None));
    let err = validate_map(&map_def).expect_err("key without kind must be rejected");
    assert!(err.to_string().contains("kind"));
}

#[test]
fn validation_rejects_kind_on_non_key_item() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 0, 0, "cookie", Some("red")));
    let err = validate_map(&map_def).expect_err("kind on non-key item must be rejected");
    assert!(err.to_string().contains("only key items"));
}

#[test]
fn validation_rejects_unknown_item_type() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 0, 0, "banana", None));
    let err = validate_map(&map_def).expect_err("unknown item type must be rejected");
    assert!(err.to_string().contains("unknown item type"));
}

#[test]
fn validation_rejects_duplicate_item_cell() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 0, 0, "cookie", None));
    map_def.items.push(item_def(0, 0, 0, "speed", None));
    let err = validate_map(&map_def).expect_err("two items on one cell must be rejected");
    assert!(err.to_string().contains("duplicates"));
}

#[test]
fn compile_rejects_item_on_floorless_cell() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 2, 2, "cookie", None));
    let err = compile_map(&map_def, sizes(), &no_nested(), &empty_kind_table(), &no_bridges())
        .expect_err("item on a floorless cell must fail");
    assert!(err.to_string().contains("floor"));
}

#[test]
fn compile_rejects_item_on_ramp_cell() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[3, 3]]), level(vec![[0, 0]]), level(vec![[3, 3]])],
        Vec::new(),
        vec![player_zone(0, 3, 3)],
        vec![ramp([0, 0], [1, 2], 1)],
    );
    map_def.items.push(item_def(1, 0, 0, "cookie", None));
    let err = compile_map(&map_def, sizes(), &no_nested(), &empty_kind_table(), &no_bridges())
        .expect_err("item on a ramp cell must fail");
    assert!(err.to_string().contains("ramp"));
}

#[test]
fn compile_resolves_key_item_barrier_kind() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.items.push(item_def(0, 0, 0, "key", Some("red")));
    let (_, config) =
        compile_map(&map_def, sizes(), &no_nested(), &red_only_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(config.placed_items.len(), 1);
    assert_eq!(
        config.placed_items[0].item_type,
        common::protocol::ItemType::Key(common::protocol::BarrierKindId(0))
    );
}

#[test]
fn validation_rejects_duplicate_barrier() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        kind: "red".into(),
    });
    map_def.levels[0].barriers.push(BarrierDef {
        c0: 1,
        r0: 0,
        c1: 0,
        r1: 0,
        kind: "green".into(),
    });
    let err = validate_map(&map_def).expect_err("duplicate barrier edge must be rejected");
    assert!(err.to_string().contains("duplicate barrier"));
}

#[test]
fn ladder_compiles_to_world_segment_and_normal() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[1, 1]]), level(vec![[1, 0]])],
        Vec::new(),
        vec![player_zone(0, 1, 1)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 1, 1, WallSide::North, 1));

    let (layout, _) =
        compile_map(&map_def, sizes(), &no_nested(), &empty_kind_table(), &no_bridges()).expect("compile");

    assert_eq!(layout.ladders.len(), 1);
    let out = layout.ladders[0];
    // 4x4 grid centered on the origin: cell (1,1) spans world -3.4..0.0 on
    // both axes; its north edge lies at z = -3.4 with the cell center at
    // x = -1.7. The segment is LADDER_WIDTH wide around that midpoint.
    let half_width = common::constants::LADDER_WIDTH / 2.0;
    assert!((out.x1 - (-1.7 - half_width)).abs() < 1e-5);
    assert!((out.x2 - (-1.7 + half_width)).abs() < 1e-5);
    assert!((out.z1 - -3.4).abs() < 1e-5);
    assert!((out.z2 - -3.4).abs() < 1e-5);
    assert_eq!((out.nx, out.nz), (0.0, -1.0));
    assert_eq!((out.level, out.levels), (0, 1));
}

#[test]
fn validation_accepts_stacked_non_overlapping_ladders() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 0, 0, WallSide::East, 1));
    map_def.ladders.push(ladder(1, 0, 0, WallSide::East, 1));

    validate_map(&map_def).expect("stacked ladders with disjoint spans should validate");
}

#[test]
fn validation_rejects_ladder_span_past_top_level() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 0, 0, WallSide::East, 2));

    let err = format!(
        "{:#}",
        validate_map(&map_def).expect_err("span past the top level must be rejected")
    );
    assert!(err.contains("does not exist"));
}

#[test]
fn validation_rejects_zero_storey_ladder() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 0, 0, WallSide::East, 0));

    let err = format!(
        "{:#}",
        validate_map(&map_def).expect_err("zero-storey ladder must be rejected")
    );
    assert!(err.contains("at least 1"));
}

#[test]
fn validation_rejects_overlapping_ladders_on_same_edge() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 0, 0, WallSide::East, 2));
    map_def.ladders.push(ladder(1, 0, 0, WallSide::East, 1));

    let err = validate_map(&map_def).expect_err("overlapping ladder spans must be rejected");
    assert!(err.to_string().contains("overlaps"));
}

#[test]
fn validation_rejects_mirrored_ladders_on_same_edge() {
    // Cell (0,0)'s east edge is cell (1,0)'s west edge; an edge holds at
    // most one ladder, so the mirrored pair is rejected as a duplicate.
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 0, 0, WallSide::East, 1));
    map_def.ladders.push(ladder(0, 1, 0, WallSide::West, 1));

    let err = validate_map(&map_def).expect_err("mirrored ladders on one edge must be rejected");
    assert!(err.to_string().contains("overlaps"));
}

#[test]
fn validation_rejects_out_of_bounds_ladder() {
    let mut map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.ladders.push(ladder(0, 5, 0, WallSide::East, 1));

    let err = format!(
        "{:#}",
        validate_map(&map_def).expect_err("out-of-bounds ladder must be rejected")
    );
    assert!(err.contains("out of grid bounds"));
}

// Simulate a player climbing every authored ladder in every shipped map:
// stand in the climb volume at the base, hold walk-speed input into the face,
// and require the ascent to gain at least one full storey. Catches authoring
// mistakes the permissive placement rules allow — most importantly a ladder
// facing the wrong way, whose climb volume sits under the very slab it should
// arrive on (the ascent stalls on the slab's underside).
#[test]
fn every_shipped_ladder_ascends_at_least_one_storey() {
    use bevy::math::Vec3;
    use common::{
        constants::TICK_SECS,
        map::Carriers,
        physics::{CharacterEnvironment, CharacterStep, CollisionWorld, step_character_movement},
        protocol::Position,
    };

    let server_gameplay =
        crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let gameplay = server_gameplay.gameplay_config();
    let physics = gameplay.player.physics();
    let maps_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../config/server/maps");
    for entry in std::fs::read_dir(maps_dir).expect("maps dir readable") {
        let path = entry.expect("maps dir entry readable").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let map_name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .expect("map file name is not UTF-8");
        // A file without a registry entry is a nested-only map, played
        // through its host and generated with it.
        let Some(map_server_config) = server_gameplay.maps.get(map_name) else {
            continue;
        };
        let map_settings = &map_server_config.settings;
        let (kind_table, bridge_table) = map_settings.kind_tables().expect("shipped kind tables rejected");
        let map_sizes = map_settings.geometry;
        let layout = crate::map::generate_map(
            map_name,
            map_sizes,
            &|nested| server_gameplay.maps.get(nested).map(|map| map.settings.geometry),
            &kind_table,
            &bridge_table,
        )
        .expect("map failed to generate")
        .layout;
        let world = CollisionWorld::from_map_layout(&layout, &kind_table);
        let carriers = Carriers::from_layout(&layout);

        for ladder in &layout.ladders {
            // A carried ladder's record is in its carrier's frame.
            let pose = carriers.pose(ladder.carrier);
            let mid_x = f32::midpoint(ladder.x1, ladder.x2);
            let mid_z = f32::midpoint(ladder.z1, ladder.z2);
            let mut pos = Position::from(pose.transform_point(Vec3::new(
                ladder.nx.mul_add(0.6, mid_x),
                ladder.y,
                ladder.nz.mul_add(0.6, mid_z),
            )));
            let mut vertical_velocity = 0.0;
            let one_storey_up = pose.translation.y + ladder.y + map_sizes.level_height - 0.05;
            let speed = map_settings.movement.player.walk_speed;
            let mut reached = false;
            for _ in 0..600 {
                let step = step_character_movement(
                    CharacterStep {
                        start: pos,
                        vertical_velocity,
                        control_velocity: Vec3::new(-ladder.nx * speed, 0.0, -ladder.nz * speed),
                        external_displacement: Vec3::ZERO,
                        delta: TICK_SECS,
                    },
                    &CharacterEnvironment {
                        collision_world: &world,
                        gravity: map_settings.movement.gravity,
                        passable_kinds: &[],
                        physics,
                        ladder_climb_ratio: map_settings.movement.ladder_climb_ratio,
                        portals: None,
                        carriers: &carriers,
                    },
                );
                pos = step.position;
                vertical_velocity = step.vertical_velocity;
                if pos.y >= one_storey_up {
                    reached = true;
                    break;
                }
            }
            assert!(
                reached,
                "{}: ladder at ({:.1}, {:.1}) level {} stalled at y={:.2} (needed {:.2}) — \
                 likely facing the wrong way (landing over the climb side)",
                path.display(),
                mid_x,
                mid_z,
                ladder.level,
                pos.y,
                one_storey_up
            );
        }
    }
}

// Every shipped carrier carries a standing player through a whole cycle:
// the feet stay on the surface at its origin at every tick. A tile's
// origin is its one cell, a room's the cell its grid is centered on.
#[test]
fn every_shipped_carrier_carries_a_standing_player_through_its_cycle() {
    use bevy::math::Vec3;
    use common::{
        constants::{CARRIER_RIDE_TOLERANCE, TICK_SECS},
        map::Carriers,
        physics::{CharacterEnvironment, CharacterStep, CollisionWorld, step_character_movement},
        protocol::{CarrierId, Position},
    };

    let server_gameplay =
        crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let gameplay = server_gameplay.gameplay_config();
    let physics = gameplay.player.physics();
    let maps_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../config/server/maps");
    let mut checked = 0;
    for entry in std::fs::read_dir(maps_dir).expect("maps dir readable") {
        let path = entry.expect("maps dir entry readable").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let map_name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .expect("map file name is not UTF-8");
        // A file without a registry entry is a nested-only map, played
        // through its host and generated with it.
        let Some(map_server_config) = server_gameplay.maps.get(map_name) else {
            continue;
        };
        let map_settings = &map_server_config.settings;
        let (kind_table, bridge_table) = map_settings.kind_tables().expect("shipped kind tables rejected");
        let map_sizes = map_settings.geometry;
        let layout = crate::map::generate_map(
            map_name,
            map_sizes,
            &|nested| server_gameplay.maps.get(nested).map(|map| map.settings.geometry),
            &kind_table,
            &bridge_table,
        )
        .expect("map failed to generate")
        .layout;
        let mut world = CollisionWorld::from_map_layout(&layout, &kind_table);
        let mut carriers = Carriers::from_layout(&layout);

        for (index, carrier) in layout.carriers.iter().enumerate() {
            let id = CarrierId(index as u16 + 1);
            carriers.advance(0);
            world.set_carrier_poses(&carriers);
            let mut pos = Position::from(carriers.pose(id).translation);
            let mut vertical_velocity = 0.0;
            let cycle = 2 * (carrier.travel_ticks + carrier.pause_ticks);
            for tick in 1..=cycle {
                carriers.advance(tick);
                world.set_carrier_poses(&carriers);
                let step = step_character_movement(
                    CharacterStep {
                        start: pos,
                        vertical_velocity,
                        control_velocity: Vec3::ZERO,
                        external_displacement: Vec3::ZERO,
                        delta: TICK_SECS,
                    },
                    &CharacterEnvironment {
                        collision_world: &world,
                        gravity: map_settings.movement.gravity,
                        passable_kinds: &[],
                        physics,
                        ladder_climb_ratio: map_settings.movement.ladder_climb_ratio,
                        portals: None,
                        carriers: &carriers,
                    },
                );
                pos = step.position;
                vertical_velocity = step.vertical_velocity;
                let surface = carriers.pose(id).translation;
                let gap = Vec3::from(pos) - surface;
                assert!(
                    gap.length() <= CARRIER_RIDE_TOLERANCE,
                    "{}: carrier {} lost its rider at tick {tick}: feet {pos:?}, surface {surface}",
                    path.display(),
                    id.0
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no shipped map has a carrier to check");
}

// === Nested maps ===

// A 3x2 room with a floor on every cell, a wall along its north edge, one
// cookie, one firework plate, a player zone on its first cell, and an actor
// zone, on two storeys.
fn room() -> MapDef {
    let mut map_def = map_with_zones(
        3,
        vec![
            level(vec![[0, 0], [1, 0], [2, 0], [0, 1], [1, 1], [2, 1]]),
            level(vec![[0, 0]]),
        ],
        vec![actor_zone(0, 1, 1)],
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.grid_rows = 2;
    map_def.levels[0].walls.push(WallDef {
        c0: 0,
        r0: 0,
        c1: 1,
        r1: 0,
        materials: FaceMaterials::uniform("test"),
    });
    map_def.items.push(ItemDef {
        level: 0,
        col: 2,
        row: 1,
        item_type: "cookie".into(),
        kind: None,
    });
    map_def.pressure_plates.push(PressurePlateDef {
        level: 0,
        col: 1,
        row: 1,
        purpose: PressurePlatePurposeDef::Firework,
    });
    map_def
}

// A 6x6 host with one floor, nesting `entries`.
fn host(entries: Vec<NestedMapDef>) -> MapDef {
    let mut map_def = map_with_zones(
        6,
        vec![level(vec![[0, 0]]), level(vec![[0, 0]]), level(vec![[0, 0]])],
        Vec::new(),
        vec![player_zone(0, 0, 0)],
        Vec::new(),
    );
    map_def.nested_maps = entries;
    map_def
}

fn tree(maps: Vec<(&str, MapDef)>) -> LoadedMaps {
    maps.into_iter().map(|(name, def)| (name.to_owned(), def)).collect()
}

fn compile_host(host: &MapDef, nested: &LoadedMaps) -> (common::protocol::MapLayout, crate::map::MapConfig) {
    compile_map(host, sizes(), nested, &empty_kind_table(), &no_bridges()).expect("host failed to compile")
}

#[test]
fn validation_rejects_nested_map_with_path_unsafe_name() {
    let map_def = host(vec![nested("../secret", 0, [2, 2], [2, 2], 0)]);
    let error = validate_map(&map_def).expect_err("path-unsafe nested name accepted");
    assert!(error.to_string().contains("nested_maps[0]"), "{error}");
}

#[test]
fn validation_rejects_nested_map_anchor_outside_the_grid() {
    let map_def = host(vec![nested("room", 0, [6, 2], [2, 2], 0)]);
    assert!(validate_map(&map_def).is_err());
}

#[test]
fn validation_rejects_nested_map_level_out_of_range() {
    let map_def = host(vec![nested("room", 3, [2, 2], [2, 2], 3)]);
    assert!(validate_map(&map_def).is_err());
}

#[test]
fn validation_rejects_non_positive_nested_map_speed() {
    let mut map_def = host(vec![nested("room", 0, [2, 2], [4, 2], 0)]);
    map_def.nested_maps[0].motion.travel_secs = 0.0;
    assert!(validate_map(&map_def).is_err());
}

#[test]
fn validation_rejects_a_non_finite_nudge() {
    let mut map_def = host(vec![nested("room", 0, [2, 2], [4, 2], 0)]);
    map_def.nested_maps[0].motion.to_nudge = [0.0, f32::NAN, 0.0];
    let error = validate_map(&map_def).expect_err("non-finite nudge accepted");
    assert!(format!("{error:#}").contains("to_nudge"), "{error:#}");
}

#[test]
fn nudges_displace_each_end_by_wall_widths_across_and_floor_thicknesses_up() {
    use common::protocol::CarrierId;

    let mut entry = nested("room", 0, [1, 3], [5, 3], 0);
    entry.motion.from_nudge = [1.0, 0.0, 0.0];
    entry.motion.to_nudge = [0.0, -2.0, 3.0];
    let plain = host(vec![nested("room", 0, [1, 3], [5, 3], 0)]);
    let (plain_layout, _) = compile_host(&plain, &tree(vec![("room", room())]));
    let (layout, _) = compile_host(&host(vec![entry]), &tree(vec![("room", room())]));
    let (plain_from, plain_to) = (
        Vec3::from(plain_layout.carriers[0].from),
        Vec3::from(plain_layout.carriers[0].to),
    );
    let carrier = layout.carriers[0];

    assert_eq!(carrier.parent, CarrierId::WORLD);
    let expected_from = plain_from + Vec3::X * WALL_THICKNESS;
    let expected_to = plain_to + Vec3::new(0.0, -2.0 * FLOOR_THICKNESS, 3.0 * WALL_THICKNESS);
    assert!(
        (Vec3::from(carrier.from) - expected_from).length() < 1e-5,
        "from {:?}",
        carrier.from
    );
    assert!(
        (Vec3::from(carrier.to) - expected_to).length() < 1e-5,
        "to {:?}",
        carrier.to
    );
}

#[test]
fn nudges_default_to_zero() {
    let entry: NestedMapDef =
        serde_json::from_str(r#"{"map": "room", "level": 0, "from": [1, 1], "to": [3, 1], "travel_secs": 2.0}"#)
            .expect("entry without nudges rejected");
    assert_eq!((entry.motion.from_nudge, entry.motion.to_nudge), ([0.0; 3], [0.0; 3]));
}

#[test]
fn travel_time_sets_the_travel_ticks_whatever_the_distance() {
    let mut short = nested("room", 0, [1, 3], [2, 3], 0);
    short.motion.travel_secs = 1.5;
    let mut long = nested("room", 0, [1, 3], [5, 3], 0);
    long.motion.travel_secs = 1.5;
    let (layout, _) = compile_host(&host(vec![short, long]), &tree(vec![("room", room())]));
    assert_eq!(layout.carriers[0].travel_ticks, 45);
    assert_eq!(layout.carriers[1].travel_ticks, 45);
}

#[test]
fn validation_accepts_a_stationary_nested_map() {
    let map_def = host(vec![nested("room", 0, [2, 2], [2, 2], 0)]);
    validate_map(&map_def).expect("a room placed once was rejected");
}

#[test]
fn validation_accepts_a_file_without_player_spawn_zones() {
    let mut map_def = room();
    map_def.player_spawn_zones.clear();
    validate_map(&map_def).expect("a nested-only file was rejected");
}

#[test]
fn validation_rejects_two_nested_maps_starting_on_one_cell() {
    let map_def = host(vec![
        nested("room", 0, [2, 2], [4, 2], 0),
        nested("room", 0, [2, 2], [2, 4], 0),
    ]);
    let error = validate_map(&map_def).expect_err("duplicate start cell accepted");
    assert!(error.to_string().contains("duplicates"), "{error}");
}

#[test]
fn nested_cell_zero_lands_on_the_parent_anchor_cell() {
    use common::{map::MapGeometry, protocol::CarrierId};

    let host_def = host(vec![nested("room", 1, [2, 3], [4, 3], 1)]);
    let (layout, _) = compile_host(&host_def, &tree(vec![("room", room())]));
    let carrier = layout.carriers[0];
    let parent = MapGeometry::new(6, 6, sizes());
    let child = MapGeometry::new(3, 2, sizes());

    assert_eq!(carrier.parent, CarrierId::WORLD);
    assert!((carrier.from.x + child.cell_center_x(0) - parent.cell_center_x(2)).abs() < 1e-5);
    assert!((carrier.from.z + child.cell_center_z(0) - parent.cell_center_z(3)).abs() < 1e-5);
    assert!((carrier.from.y - parent.level_y(1)).abs() < 1e-5);
    assert!((carrier.to.x + child.cell_center_x(0) - parent.cell_center_x(4)).abs() < 1e-5);
    assert_eq!((carrier.level, carrier.levels), (1, 0));
}

#[test]
fn nested_records_stay_in_their_own_frame_and_carry_their_id() {
    use common::{map::MapGeometry, protocol::CarrierId};

    let host_def = host(vec![nested("room", 0, [2, 2], [2, 2], 0)]);
    let (layout, _) = compile_host(&host_def, &tree(vec![("room", room())]));
    let child = MapGeometry::new(3, 2, sizes());
    let room_walls: Vec<_> = layout
        .walls
        .iter()
        .filter(|wall| wall.carrier == CarrierId(1))
        .collect();
    assert_eq!(room_walls.len(), 1);
    // The room's north wall runs along its own grid line z = row 0, not the host's.
    assert!(
        (room_walls[0].z1 - child.cell_to_world_z(0)).abs() < 1e-5,
        "wall at {:?}",
        room_walls[0]
    );
    assert!(layout.floors.iter().any(|floor| floor.carrier == CarrierId(1)));
    assert!(layout.floors.iter().any(|floor| floor.carrier.is_world()));
    assert_eq!(layout.pressure_plates.len(), 1);
    assert_eq!(layout.pressure_plates[0].carrier, CarrierId(1));
    assert!((layout.pressure_plates[0].center_x - child.cell_center_x(1)).abs() < 1e-5);
}

#[test]
fn nested_kinds_resolve_against_the_root_tables_and_an_unknown_kind_names_the_nested_map() {
    let mut keyed_room = room();
    keyed_room.levels[0].barriers.push(BarrierDef {
        c0: 1,
        r0: 0,
        c1: 1,
        r1: 1,
        kind: "red".into(),
    });
    let host_def = host(vec![nested("room", 0, [2, 2], [2, 2], 0)]);
    let nested_maps = tree(vec![("room", keyed_room)]);

    let (layout, _) = compile_map(&host_def, sizes(), &nested_maps, &red_only_kind_table(), &no_bridges())
        .expect("a nested barrier of a root kind failed to compile");
    assert_eq!(layout.barriers.len(), 1);

    let error = compile_map(&host_def, sizes(), &nested_maps, &empty_kind_table(), &no_bridges())
        .expect_err("an unknown nested kind compiled");
    assert!(format!("{error:#}").contains("nested map \"room\""), "{error:#}");
}

#[test]
fn a_doubly_nested_carrier_is_parented_to_its_nesting_carrier_and_ids_come_parent_first() {
    use common::protocol::CarrierId;

    let mut middle = room();
    middle.nested_maps.push(nested("inner", 0, [1, 0], [1, 0], 0));
    let host_def = host(vec![
        nested("middle", 0, [2, 2], [2, 2], 0),
        nested("inner", 0, [0, 4], [0, 4], 0),
    ]);
    let (layout, config) = compile_host(&host_def, &tree(vec![("middle", middle), ("inner", room())]));

    // middle = 1, its inner = 2, the host's own inner = 3.
    assert_eq!(layout.carriers.len(), 3);
    assert_eq!(layout.carriers[0].parent, CarrierId::WORLD);
    assert_eq!(layout.carriers[1].parent, CarrierId(1));
    assert_eq!(layout.carriers[2].parent, CarrierId::WORLD);
    assert_eq!(config.grids.len(), 4);
    for (index, grid) in config.grids.iter().enumerate() {
        assert_eq!(grid.carrier, CarrierId(index as u16));
    }
    assert_eq!(config.grid(CarrierId(2)).geometry.grid_cols, 3);
}

#[test]
fn nested_actor_spawn_zones_carry_their_carrier() {
    use common::protocol::CarrierId;

    let host_def = host(vec![nested("room", 0, [2, 2], [2, 2], 0)]);
    let (_, config) = compile_host(&host_def, &tree(vec![("room", room())]));
    assert_eq!(config.actor_spawn_zones.len(), 1);
    assert_eq!(config.actor_spawn_zones[0].carrier, CarrierId(1));
    assert_eq!(config.actor_spawn_zones[0].cols, [1, 2]);
    assert_eq!(config.actor_spawn_zones[0].rows, [1, 2]);
}

#[test]
fn nested_player_spawn_zones_items_and_plates_carry_their_carrier() {
    use common::protocol::CarrierId;

    let host_def = host(vec![nested("room", 0, [2, 2], [2, 2], 0)]);
    let (_, config) = compile_host(&host_def, &tree(vec![("room", room())]));
    assert_eq!(config.player_spawn_zones.len(), 2);
    assert_eq!(config.player_spawn_zones[1].carrier, CarrierId(1));
    assert_eq!(config.placed_items.len(), 1);
    assert_eq!(config.placed_items[0].carrier, CarrierId(1));
    assert_eq!(config.pressure_plates.len(), 1);
    assert_eq!(config.pressure_plates[0].carrier, CarrierId(1));
}
