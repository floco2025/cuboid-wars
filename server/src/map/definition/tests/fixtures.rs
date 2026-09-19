pub(super) use bevy::math::Vec3;
pub(super) use common::{
    map::Carriers,
    physics::{CollisionWorld, compute_portal_placement},
    protocol::{
        CarrierId, FaceMaterials, FieldDef, FieldTable, HexColor, MapLayout, MapSettings, Position, RampDirection,
        RampShape, SwitchDef, SwitchId, SwitchTable, TERRAIN_MATERIAL, TextureSettings,
    },
};

pub(super) use super::super::compile_map;
pub(super) use crate::{
    actors::navigation::NavGraph,
    map::MapConfig,
    test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS, map_settings, sizes},
};
pub(super) use map_core::{
    load::{LoadedMaps, canonicalize, validate_map},
    schema::{
        ActorSpawnZoneDef, BarrierDef, EraserDef, FloorDef, ItemDef, LadderDef, LevelDef, LightBridgeDef, MapDef,
        MotionDef, NestedMapDef, PressurePlateDef, RampDef, TerrainDef, WallDef, WallSide, ZoneDef,
    },
};

// The switch every plate in these tests may name: one per field, named after
// it, plus `fireworks`. Every field starts on, and `red` alone is a door on
// its switch.
pub(crate) const FIREWORKS: &str = "fireworks";

pub(crate) fn compile_settings(kinds: &FieldTable) -> MapSettings {
    let switch_ids = kinds.ids().iter().cloned().chain([FIREWORKS.to_owned()]);
    let field_def = |id: &String| FieldDef {
        id: id.clone(),
        color: HexColor([0; 3]),
        switch: (id == "red").then(|| id.clone()),
        initially_on: true,
    };
    MapSettings {
        switches: switch_ids
            .map(|id| SwitchDef {
                id,
                color: None,
                policy: Default::default(),
            })
            .collect(),
        fields: kinds.ids().iter().map(field_def).collect(),
        ..map_settings()
    }
}

pub(crate) fn switch_table(kinds: &FieldTable) -> SwitchTable {
    SwitchTable::from_switch_defs(&compile_settings(kinds).switches).expect("test switch table rejected")
}

pub(crate) fn switch_id(kinds: &FieldTable, switch: &str) -> SwitchId {
    switch_table(kinds)
        .index_of(switch)
        .expect("test switch missing from its table")
}

// `compile_map` with the test settings for these tables.
pub(crate) fn compile_with(
    map: &MapDef,
    nested: &LoadedMaps,
    kinds: &FieldTable,
) -> anyhow::Result<(MapLayout, MapConfig)> {
    compile_with_settings(map, nested, &compile_settings(kinds))
}

pub(crate) fn compile_with_settings(
    map: &MapDef,
    nested: &LoadedMaps,
    settings: &MapSettings,
) -> anyhow::Result<(MapLayout, MapConfig)> {
    let fields = settings.field_table().expect("test field table rejected");
    let switches = SwitchTable::from_switch_defs(&settings.switches).expect("test switch table rejected");
    compile_map(map, 30, settings, nested, &fields, &switches)
}

pub(crate) fn plate_def(level: u32, col: i32, row: i32, switch: &str) -> PressurePlateDef {
    PressurePlateDef {
        level,
        col,
        row,
        switch: switch.into(),
    }
}

pub(crate) fn empty_kind_table() -> FieldTable {
    FieldTable::default()
}

pub(crate) fn skyway_kind_table() -> FieldTable {
    FieldTable::from_ids(vec!["skyway".into()]).expect("one-kind table rejected")
}

pub(crate) fn red_only_kind_table() -> FieldTable {
    FieldTable::from_ids(vec!["red".into()]).expect("known-good")
}

pub(crate) fn three_kind_table() -> FieldTable {
    FieldTable::from_ids(vec!["red".into(), "blue".into(), "green".into()]).expect("known-good")
}

pub(crate) fn floor_def(col: i32, row: i32) -> FloorDef {
    FloorDef {
        col,
        row,
        materials: FaceMaterials::uniform("test"),
    }
}

pub(crate) fn cell_def(col: i32, row: i32) -> TerrainDef {
    TerrainDef {
        col,
        row,
        materials: FaceMaterials {
            top: TERRAIN_MATERIAL.into(),
            bottom: "test".into(),
            north: "test".into(),
            south: "test".into(),
            east: "test".into(),
            west: "test".into(),
        },
    }
}

pub(crate) fn bridge_def(col: i32, row: i32) -> LightBridgeDef {
    LightBridgeDef {
        col,
        row,
        field: "skyway".into(),
    }
}

// One floor at (0, 0) plus the bridge cells named, on a 4x4 grid.
pub(crate) fn map_with_bridges(cells: &[[i32; 2]]) -> MapDef {
    let mut map_def = map_with_zones(4, vec![level(vec![[0, 0]])], Vec::new(), Vec::new());
    map_def.levels[0].light_bridges = cells.iter().map(|[c, r]| bridge_def(*c, *r)).collect();
    map_def
}

pub(crate) fn level(floors: Vec<[i32; 2]>) -> LevelDef {
    level_with_inaccessible(floors, Vec::new())
}

pub(crate) fn level_with_inaccessible(floors: Vec<[i32; 2]>, inaccessible_floors: Vec<[i32; 2]>) -> LevelDef {
    LevelDef {
        name: None,
        floors: floors.into_iter().map(|[c, r]| floor_def(c, r)).collect(),
        inaccessible_floors: inaccessible_floors.into_iter().map(|[c, r]| floor_def(c, r)).collect(),
        terrain: Vec::new(),
        walls: Vec::new(),
        barriers: Vec::new(),
        erasers: Vec::new(),
        light_bridges: Vec::new(),
        lights: Vec::new(),
    }
}

pub(crate) fn actor_zone(level: u32, col: i32, row: i32) -> ActorSpawnZoneDef {
    ActorSpawnZoneDef {
        initially_on: true,

        level,

        levels: 1,

        roam_distance: 0.0,
        cols: [col, col + 1],
        rows: [row, row + 1],
        kind: "actor".into(),
        count: vec![1],
        respawn_secs: Some(90.0),
        beam_in_secs: 0.0,
        switch: None,
        until_checkpoint: None,
        on_checkpoint: None,
    }
}

pub(crate) fn ramp(cols: [i32; 2], rows: [i32; 2], direction: RampDirection, lower_level: u32) -> RampDef {
    RampDef {
        lower_level,
        levels: 1,
        cols,
        rows,
        direction,
        shape: RampShape::Solid,
        materials: FaceMaterials::uniform("test"),
    }
}

pub(crate) fn map_with_zones(
    grid: i32,
    levels: Vec<LevelDef>,
    actor_spawn_zones: Vec<ActorSpawnZoneDef>,
    ramps: Vec<RampDef>,
) -> MapDef {
    MapDef {
        grid_cols: grid,
        grid_rows: grid,
        actor_spawn_zones,
        checkpoints: Vec::new(),
        items: Vec::new(),
        pressure_plates: Vec::new(),
        levels,
        ramps,
        ladders: Vec::new(),
        nested_maps: Vec::new(),
        switches: Vec::new(),
        fields: Vec::new(),
        fireworks: None,
        nested_geometry: Default::default(),
    }
}

pub(crate) fn no_nested() -> LoadedMaps {
    LoadedMaps::default()
}

pub(crate) fn ladder(lower_level: u32, col: i32, row: i32, side: WallSide, levels: u32) -> LadderDef {
    LadderDef {
        lower_level,
        col,
        row,
        side,
        levels,
    }
}

pub(crate) fn item_def(level: u32, col: i32, row: i32, item_type: &str, field: Option<&str>) -> ItemDef {
    ItemDef {
        level,
        col,
        row,
        item_type: item_type.to_owned(),
        field: field.map(str::to_owned),
    }
}
