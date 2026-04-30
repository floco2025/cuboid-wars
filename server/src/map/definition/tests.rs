use super::*;

fn level(floors: Vec<[i32; 2]>) -> LevelDef {
    level_with_inaccessible(floors, Vec::new())
}

fn level_with_inaccessible(floors: Vec<[i32; 2]>, inaccessible_floors: Vec<[i32; 2]>) -> LevelDef {
    LevelDef {
        name: None,
        floors,
        inaccessible_floors,
        walls: Vec::new(),
    }
}

const fn spawn(level: u32, col: i32, row: i32) -> PlayerSpawnDef {
    PlayerSpawnDef { level, col, row }
}

fn assets() -> MaterialRules {
    MaterialRules::load_default().expect("default asset rules should load")
}

#[test]
fn validation_rejects_spawn_field_without_floor_on_its_level() {
    let map_def = MapDef {
        grid_cols: 4,
        grid_rows: 4,
        player_spawn_fields: vec![spawn(1, 0, 0)],
        levels: vec![level(vec![[0, 0]]), level(vec![[1, 0]])],
        ramps: Vec::new(),
    };

    let err = validate_map(&map_def).expect_err("spawn field must be a floor on its level");
    assert!(err.to_string().contains("not a floor on level 1"));
}

#[test]
fn validation_accepts_spawn_field_on_higher_level_floor() {
    let map_def = MapDef {
        grid_cols: 4,
        grid_rows: 4,
        player_spawn_fields: vec![spawn(1, 0, 0)],
        levels: vec![level(vec![[1, 0]]), level(vec![[0, 0]])],
        ramps: Vec::new(),
    };

    validate_map(&map_def).expect("spawn field should be allowed on any level floor");
}

#[test]
fn validation_rejects_spawn_field_on_inaccessible_floor() {
    let map_def = MapDef {
        grid_cols: 4,
        grid_rows: 4,
        player_spawn_fields: vec![spawn(0, 1, 0)],
        levels: vec![level_with_inaccessible(vec![[0, 0]], vec![[1, 0]])],
        ramps: Vec::new(),
    };

    let err = validate_map(&map_def).expect_err("spawn field must require a regular floor");
    assert!(err.to_string().contains("not a floor on level 0"));
}

#[test]
fn inaccessible_floor_emits_physical_slab_but_not_regular_floor() {
    let map_def = MapDef {
        grid_cols: 4,
        grid_rows: 4,
        player_spawn_fields: vec![spawn(0, 0, 0)],
        levels: vec![level_with_inaccessible(vec![[0, 0]], vec![[2, 0]])],
        ramps: Vec::new(),
    };

    let (layout, config) = compile_map(&map_def, &assets());
    let inaccessible_cell = config.levels[0].cells.rows[0][2];
    assert!(!inaccessible_cell.has_floor);
    assert!(inaccessible_cell.has_floor_slab);

    let x = (2.5_f32).mul_add(GRID_CELL_SIZE, -(MAP_WIDTH / 2.0));
    let z = (0.5_f32).mul_add(GRID_CELL_SIZE, -(MAP_DEPTH / 2.0));
    assert!(layout.floors.iter().any(|floor| {
        let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
        min_x <= x && x <= max_x && min_z <= z && z <= max_z
    }));
}

#[test]
fn validation_rejects_spawn_field_on_same_level_ramp() {
    let map_def = MapDef {
        grid_cols: 4,
        grid_rows: 4,
        player_spawn_fields: vec![spawn(1, 0, 0)],
        levels: vec![level(vec![[3, 3]]), level(vec![[0, 0]]), level(vec![[3, 3]])],
        ramps: vec![RampDef {
            low: [0, 0],
            high: [1, 2],
            lower_level: 1,
        }],
    };

    let err = validate_map(&map_def).expect_err("spawn field must not overlap ramp footprint");
    assert!(err.to_string().contains("overlaps a ramp on level 1"));
}
