pub(super) use bevy::math::Vec3;
pub(super) use common::{
    map::Carriers,
    physics::{CollisionWorld, compute_portal_placement},
    protocol::{BarrierKindTable, BridgeKindId, BridgeKindTable, CarrierId, FaceMaterials, Position, TextureSettings},
};

pub(super) use super::super::{
    compile_map,
    load::LoadedMaps,
    schema::{
        ActorSpawnZoneDef, BarrierDef, CellDef, EraserDef, FloorDef, ItemDef, LadderDef, LevelDef, LightBridgeDef,
        MapDef, MotionDef, NestedMapDef, PlayerSpawnZoneDef, PressurePlateDef, PressurePlatePurposeDef, RampDef,
        WallDef, WallSide,
    },
    validation::validate_map,
};
pub(super) use crate::{
    actors::navigation::NavGraph,
    map::MapConfig,
    test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS, sizes},
};

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
        level,
        cols: [col, col + 1],
        rows: [row, row + 1],
        kind: "actor".into(),
        count: 1,
    }
}

pub(crate) fn player_zone(level: u32, col: i32, row: i32) -> PlayerSpawnZoneDef {
    PlayerSpawnZoneDef {
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
