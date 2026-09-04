use super::{
    compile_map,
    schema::{
        ActorSpawnZoneDef, BarrierDef, CellDef, FloorDef, ItemDef, LadderDef, LevelDef, LightBridgeDef, MapDef,
        PlayerSpawnZoneDef, PressurePlateDef, PressurePlatePurposeDef, RampDef, WallDef, WallSide,
    },
    validation::validate_map,
};
use crate::{
    map::material_rules::MaterialRules,
    test_geometry::{LEVEL_HEIGHT, sizes},
};
use common::protocol::{BarrierKindTable, BridgeKindTable, FaceMaterials};

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
    }
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

fn assets() -> MaterialRules {
    let config =
        crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let map_def =
        super::load_map(&crate::map::generation::map_path(&config.default_map)).expect("default map should load");
    MaterialRules::from_def(&map_def, sizes())
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

    let (layout, config, geometry) =
        compile_map(&map_def, sizes(), &assets(), &empty_kind_table(), &no_bridges()).expect("compile");
    let inaccessible_cell = config.levels[0].cells.rows[0][2];
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
fn validation_rejects_missing_player_spawn_zones() {
    let map_def = map_with_zones(
        4,
        vec![level(vec![[0, 0]])],
        vec![actor_zone(0, 0, 0)],
        Vec::new(),
        Vec::new(),
    );

    let err = validate_map(&map_def).expect_err("must require at least one player zone");
    assert!(err.to_string().contains("player_spawn_zones"));
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
    let (layout, _, _) =
        compile_map(&map_def, sizes(), &assets(), &red_only_kind_table(), &no_bridges()).expect("compile");
    assert_eq!(layout.barriers.len(), 1);
    assert_eq!(layout.barriers[0].kind, common::protocol::BarrierKindId(0));
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

    let (_, config, _) =
        compile_map(&map_def, sizes(), &assets(), &three_kind_table(), &no_bridges()).expect("compile");
    let barrier_edges = &config.levels[0].barrier_edges;
    assert!(
        !barrier_edges.vertical[0][1],
        "pressure-plate (red) barrier must be treated as open for nav"
    );
    assert!(barrier_edges.vertical[0][2], "non-plate (blue) barrier must block nav");
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

    let (layout, config, _) =
        compile_map(&map_def, sizes(), &assets(), &three_kind_table(), &no_bridges()).expect("compile");
    assert!(
        config.levels[0].barrier_edges.vertical[0][1],
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
    let map_def = map_with_bridges(&[[1, 0], [2, 0], [1, 1], [2, 1]]);

    let (layout, _, geometry) = compile_map(
        &map_def,
        sizes(),
        &assets(),
        &empty_kind_table(),
        &skyway_bridge_table(),
    )
    .expect("compile");

    assert_eq!(layout.light_bridges.len(), 1, "a 2x2 block is one collider");
    let bridge = layout.light_bridges[0];
    assert_eq!(bridge.kind, common::protocol::BridgeKindId(0));
    assert_eq!(bridge.level, 0);
    assert!((bridge.y - 0.0).abs() < 1e-4, "level 0 stands at y = 0");
    let (min_x, max_x, min_z, max_z) = bridge.bounds_xz();
    assert!((min_x - geometry.cell_to_world_x(1)).abs() < 1e-4);
    assert!((max_x - geometry.cell_to_world_x(3)).abs() < 1e-4);
    assert!((min_z - geometry.cell_to_world_z(0)).abs() < 1e-4);
    assert!((max_z - geometry.cell_to_world_z(2)).abs() < 1e-4);
}

#[test]
fn compile_rejects_unknown_bridge_kind() {
    let mut map_def = map_with_bridges(&[[1, 0]]);
    map_def.levels[0].light_bridges[0].kind = "magenta".into();

    let err = compile_map(
        &map_def,
        sizes(),
        &assets(),
        &empty_kind_table(),
        &skyway_bridge_table(),
    )
    .err()
    .expect("unknown bridge kind must fail");
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
    let err = compile_map(&map_def, sizes(), &assets(), &red_only_kind_table(), &no_bridges())
        .err()
        .expect("unknown kind must fail");
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
    let (layout, _, _) =
        compile_map(&map_def, sizes(), &assets(), &three_kind_table(), &no_bridges()).expect("compile");
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
    let (layout, _, _) =
        compile_map(&map_def, sizes(), &assets(), &empty_kind_table(), &no_bridges()).expect("compile");
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
    let (layout, _, geometry) =
        compile_map(&map_def, sizes(), &assets(), &empty_kind_table(), &no_bridges()).expect("compile");
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
    let (layout, _, _) =
        compile_map(&map_def, sizes(), &assets(), &empty_kind_table(), &no_bridges()).expect("compile");
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
    let err = compile_map(&map_def, sizes(), &assets(), &empty_kind_table(), &no_bridges())
        .err()
        .expect("item on a floorless cell must fail");
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
    let err = compile_map(&map_def, sizes(), &assets(), &empty_kind_table(), &no_bridges())
        .err()
        .expect("item on a ramp cell must fail");
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
    let (_, config, _) =
        compile_map(&map_def, sizes(), &assets(), &red_only_kind_table(), &no_bridges()).expect("compile");
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

    let (layout, _, _) =
        compile_map(&map_def, sizes(), &assets(), &empty_kind_table(), &no_bridges()).expect("compile");

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
    use common::constants::TICK_SECS;
    use common::physics::{CharacterEnvironment, CharacterStep, CollisionWorld, step_character_movement};
    use common::protocol::Position;

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
        let map_settings = &server_gameplay
            .maps
            .get(map_name)
            .expect("shipped map missing from server gameplay config")
            .settings;
        let map_def = super::load_map(&path).expect("map file should load");
        let (kind_table, bridge_table) = map_settings.kind_tables().expect("shipped kind tables rejected");
        let map_sizes = map_settings.geometry;
        let assets = MaterialRules::from_def(&map_def, map_sizes);
        let (layout, _, _) =
            compile_map(&map_def, map_sizes, &assets, &kind_table, &bridge_table).expect("map failed to compile");
        let world = CollisionWorld::from_map_layout(&layout, &kind_table);

        for ladder in &layout.ladders {
            let mid_x = f32::midpoint(ladder.x1, ladder.x2);
            let mid_z = f32::midpoint(ladder.z1, ladder.z2);
            let mut pos = Position {
                x: ladder.nx.mul_add(0.6, mid_x),
                y: ladder.y,
                z: ladder.nz.mul_add(0.6, mid_z),
            };
            let mut vertical_velocity = 0.0;
            let one_storey_up = ladder.y + map_sizes.level_height - 0.05;
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
