use serde::Deserialize;

// Serde silently ignores unknown JSON fields by default, so the per-segment
// material strings (`top`/`bottom`/.../`all`) on each floor / wall / ramp
// record are dropped here without being listed. They're parsed separately by
// `crate::map::material_rules` and threaded through compile to populate
// `MapLayout`'s parallel `*_materials` vectors.

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
    #[serde(default)]
    pub(crate) cookie_spawn_zones: Vec<CookieSpawnZoneDef>,
    pub(crate) levels: Vec<LevelDef>,
    #[serde(default)]
    pub(crate) ramps: Vec<RampDef>,
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
    #[serde(default)]
    pub(crate) barriers: Vec<BarrierDef>,
    #[serde(default)]
    pub(crate) lights: Vec<WallLightDef>,
}

// Editor-authored wall light. Identifies a `(cell, side)` pair on this level;
// the runtime turns each one into a `WallLight { pos, yaw }`.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub(crate) struct WallLightDef {
    pub(crate) col: i32,
    pub(crate) row: i32,
    pub(crate) side: WallSide,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub(crate) enum WallSide {
    #[serde(rename = "N")]
    North,
    #[serde(rename = "S")]
    South,
    #[serde(rename = "E")]
    East,
    #[serde(rename = "W")]
    West,
}

// Per-face materials live on each segment in JSON but are loaded by the
// `material_rules` pipeline, not the server-side mesh generator. Serde's
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
pub(crate) struct BarrierDef {
    pub(crate) c0: i32,
    pub(crate) r0: i32,
    pub(crate) c1: i32,
    pub(crate) r1: i32,
    pub(crate) color: BarrierColorDef,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub(crate) enum BarrierColorDef {
    #[serde(rename = "red")]
    Red,
    #[serde(rename = "blue")]
    Blue,
    #[serde(rename = "green")]
    Green,
    #[serde(rename = "yellow")]
    Yellow,
}

impl BarrierColorDef {
    pub(crate) fn to_protocol(self) -> common::protocol::BarrierColor {
        match self {
            Self::Red => common::protocol::BarrierColor::Red,
            Self::Blue => common::protocol::BarrierColor::Blue,
            Self::Green => common::protocol::BarrierColor::Green,
            Self::Yellow => common::protocol::BarrierColor::Yellow,
        }
    }
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

// Cells where cookies are eligible to spawn. Cookies only appear inside one
// of these zones; outside, the floor is cookie-free.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct CookieSpawnZoneDef {
    pub(crate) level: u32,
    pub(crate) cols: [i32; 2],
    pub(crate) rows: [i32; 2],
}
