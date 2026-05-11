use super::{
    compile_map,
    schema::{ActorSpawnZoneDef, FloorDef, LevelDef, MapDef, PlayerSpawnZoneDef, RampDef},
    validation::validate_map,
};
use crate::map::material_rules::MaterialRules;
use common::constants::GRID_CELL_SIZE;

fn floor_def(col: i32, row: i32) -> FloorDef {
    FloorDef { col, row }
}

fn level(floors: Vec<[i32; 2]>) -> LevelDef {
    level_with_inaccessible(floors, Vec::new())
}

fn level_with_inaccessible(floors: Vec<[i32; 2]>, inaccessible_floors: Vec<[i32; 2]>) -> LevelDef {
    LevelDef {
        name: None,
        floors: floors.into_iter().map(|[c, r]| floor_def(c, r)).collect(),
        inaccessible_floors: inaccessible_floors.into_iter().map(|[c, r]| floor_def(c, r)).collect(),
        walls: Vec::new(),
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
    RampDef { low, high, lower_level }
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
        cookie_spawn_zones: Vec::new(),
        levels,
        ramps,
    }
}

fn assets() -> MaterialRules {
    MaterialRules::load_default().expect("default asset rules should load")
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

    let (layout, config, geometry) = compile_map(&map_def, &assets());
    let inaccessible_cell = config.levels[0].cells.rows[0][2];
    assert!(!inaccessible_cell.has_floor);
    assert!(inaccessible_cell.has_floor_slab);

    let x = geometry.cell_to_world_x(2) + GRID_CELL_SIZE / 2.0;
    let z = geometry.cell_to_world_z(0) + GRID_CELL_SIZE / 2.0;
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
