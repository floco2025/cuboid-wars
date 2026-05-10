use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct MapFile {
    pub(crate) version: u32,
    pub(crate) map: MapDef,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MapDef {
    pub(crate) grid_cols: i32,
    pub(crate) grid_rows: i32,
    #[serde(default)]
    pub(crate) actor_spawn_zones: Vec<ActorSpawnZoneDef>,
    #[serde(default)]
    pub(crate) player_spawn_zones: Vec<PlayerSpawnZoneDef>,
    pub(crate) levels: Vec<LevelDef>,
    #[serde(default)]
    pub(crate) ramps: Vec<RampDef>,
    // Items aren't part of the geometry primitives the server consumes; their
    // materials are loaded separately by `MaterialRules`. Accept the field so
    // serde doesn't reject it; we don't read it here.
    #[serde(default, rename = "item_materials")]
    pub(crate) _item_materials: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LevelDef {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) floors: Vec<FloorDef>,
    #[serde(default)]
    pub(crate) inaccessible_floors: Vec<FloorDef>,
    #[serde(default)]
    pub(crate) walls: Vec<WallDef>,
}

// Per-face materials live on each segment in JSON but are loaded by the
// `MaterialRules` pipeline, not the server-side mesh generator. Serde's
// default behavior silently ignores the extra material keys, so we don't
// declare them here.
#[derive(Debug, Deserialize)]
pub(crate) struct FloorDef {
    pub(crate) col: i32,
    pub(crate) row: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WallDef {
    pub(crate) c0: i32,
    pub(crate) r0: i32,
    pub(crate) c1: i32,
    pub(crate) r1: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RampDef {
    pub(crate) low: [i32; 2],
    pub(crate) high: [i32; 2],
    pub(crate) lower_level: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct ActorSpawnZoneDef {
    pub(crate) level: u32,
    pub(crate) cols: [i32; 2],
    pub(crate) rows: [i32; 2],
    pub(crate) kind: String,
    pub(crate) count: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct PlayerSpawnZoneDef {
    pub(crate) level: u32,
    pub(crate) cols: [i32; 2],
    pub(crate) rows: [i32; 2],
}
