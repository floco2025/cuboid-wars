pub(super) use bevy::math::Vec3;
pub(super) use common::{
    map::Carriers,
    physics::{CollisionWorld, compute_portal_placement},
    protocol::{
        BarrierKindTable, BridgeKindTable, CarrierId, FaceMaterials, HexColor, KindDef, MapLayout, MapSettings,
        Position, SwitchDef, SwitchId, SwitchTable, TextureSettings,
    },
};

pub(super) use super::super::{
    compile_map,
    load::LoadedMaps,
    schema::{
        ActorSpawnZoneDef, BarrierDef, CellDef, EraserDef, FloorDef, ItemDef, LadderDef, LevelDef, LightBridgeDef,
        MapDef, MotionDef, NestedMapDef, PressurePlateDef, RampDef, WallDef, WallSide, ZoneDef,
    },
    validation::{canonicalize, validate_map},
};
pub(super) use crate::{
    actors::navigation::NavGraph,
    map::MapConfig,
    test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS, map_settings, sizes},
};

// The switch every plate in these tests may name: one per barrier and
// bridge kind, named after it, plus `fireworks`.
pub(crate) const FIREWORKS: &str = "fireworks";

pub(crate) fn compile_settings(kinds: &BarrierKindTable, bridges: &BridgeKindTable) -> MapSettings {
    let switch_ids = kinds
        .ids()
        .iter()
        .chain(bridges.ids())
        .cloned()
        .chain([FIREWORKS.to_owned()]);
    let kind_def = |id: &String| KindDef {
        id: id.clone(),
        color: HexColor([0; 3]),
    };
    MapSettings {
        switches: switch_ids
            .map(|id| SwitchDef {
                id,
                plate_color: None,
                policy: Default::default(),
            })
            .collect(),
        barrier_kinds: kinds.ids().iter().map(kind_def).collect(),
        bridge_kinds: bridges.ids().iter().map(kind_def).collect(),
        ..map_settings()
    }
}

pub(crate) fn switch_table(kinds: &BarrierKindTable, bridges: &BridgeKindTable) -> SwitchTable {
    SwitchTable::from_switch_defs(&compile_settings(kinds, bridges).switches).expect("test switch table rejected")
}

pub(crate) fn switch_id(kinds: &BarrierKindTable, bridges: &BridgeKindTable, switch: &str) -> SwitchId {
    switch_table(kinds, bridges)
        .index_of(switch)
        .expect("test switch missing from its table")
}

// `compile_map` with the test settings for these tables.
pub(crate) fn compile_with(
    map: &MapDef,
    nested: &LoadedMaps,
    kinds: &BarrierKindTable,
    bridges: &BridgeKindTable,
) -> anyhow::Result<(MapLayout, MapConfig)> {
    compile_map(
        map,
        30,
        &compile_settings(kinds, bridges),
        nested,
        kinds,
        bridges,
        &switch_table(kinds, bridges),
    )
}

pub(crate) fn plate_def(level: u32, col: i32, row: i32, switch: &str) -> PressurePlateDef {
    PressurePlateDef {
        level,
        col,
        row,
        switch: switch.into(),
    }
}

pub(crate) fn empty_kind_table() -> BarrierKindTable {
    BarrierKindTable::default()
}

pub(crate) fn no_bridges() -> BridgeKindTable {
    BridgeKindTable::default()
}

pub(crate) fn skyway_bridge_table() -> BridgeKindTable {
    BridgeKindTable::from_ids(vec!["skyway".into()]).expect("one-kind bridge table rejected")
}

pub(crate) fn red_only_kind_table() -> BarrierKindTable {
    BarrierKindTable::from_ids(vec!["red".into()]).expect("known-good")
}

pub(crate) fn three_kind_table() -> BarrierKindTable {
    BarrierKindTable::from_ids(vec!["red".into(), "blue".into(), "green".into()]).expect("known-good")
}

pub(crate) fn floor_def(col: i32, row: i32) -> FloorDef {
    FloorDef {
        col,
        row,
        materials: FaceMaterials::uniform("test"),
    }
}

pub(crate) fn cell_def(col: i32, row: i32) -> CellDef {
    CellDef { col, row }
}

pub(crate) fn bridge_def(col: i32, row: i32) -> LightBridgeDef {
    LightBridgeDef {
        switch: None,
        switch_inverted: false,

        col,
        row,
        kind: "skyway".into(),
    }
}

// One floor at (0, 0) plus the bridge cells named, on a 4x4 grid.
pub(crate) fn map_with_bridges(cells: &[[i32; 2]]) -> MapDef {
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

pub(crate) fn level(floors: Vec<[i32; 2]>) -> LevelDef {
    level_with_inaccessible(floors, Vec::new())
}

pub(crate) fn level_with_inaccessible(floors: Vec<[i32; 2]>, inaccessible_floors: Vec<[i32; 2]>) -> LevelDef {
    LevelDef {
        name: None,
        floors: floors.into_iter().map(|[c, r]| floor_def(c, r)).collect(),
        inaccessible_floors: inaccessible_floors.into_iter().map(|[c, r]| floor_def(c, r)).collect(),
        grass: Vec::new(),
        walls: Vec::new(),
        barriers: Vec::new(),
        erasers: Vec::new(),
        light_bridges: Vec::new(),
        lights: Vec::new(),
    }
}

pub(crate) fn actor_zone(level: u32, col: i32, row: i32) -> ActorSpawnZoneDef {
    ActorSpawnZoneDef {
        switch_inverted: false,

        level,
        cols: [col, col + 1],
        rows: [row, row + 1],
        kind: "actor".into(),
        count: 1,
        switch: None,
    }
}

pub(crate) fn player_zone(level: u32, col: i32, row: i32) -> ZoneDef {
    ZoneDef {
        level,
        cols: [col, col + 1],
        rows: [row, row + 1],
    }
}

pub(crate) fn ramp(low: [i32; 2], high: [i32; 2], lower_level: u32) -> RampDef {
    RampDef {
        low,
        high,
        lower_level,
        materials: FaceMaterials::uniform("test"),
    }
}

pub(crate) fn map_with_zones(
    grid: i32,
    levels: Vec<LevelDef>,
    actor_spawn_zones: Vec<ActorSpawnZoneDef>,
    player_spawn_zones: Vec<ZoneDef>,
    ramps: Vec<RampDef>,
) -> MapDef {
    MapDef {
        grid_cols: grid,
        grid_rows: grid,
        actor_spawn_zones,
        player_spawn_zones,
        checkpoints: Vec::new(),
        items: Vec::new(),
        pressure_plates: Vec::new(),
        levels,
        ramps,
        ladders: Vec::new(),
        nested_maps: Vec::new(),
        switch_kinds: Vec::new(),
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

pub(crate) fn item_def(level: u32, col: i32, row: i32, item_type: &str, kind: Option<&str>) -> ItemDef {
    ItemDef {
        level,
        col,
        row,
        item_type: item_type.to_owned(),
        kind: kind.map(str::to_owned),
    }
}
