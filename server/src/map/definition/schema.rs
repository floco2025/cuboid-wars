use serde::Deserialize;

use common::{constants::MOVING_FLOOR_INSET_FRACTION, protocol::FaceMaterials};

#[derive(Debug, Deserialize)]
pub(crate) struct MapFile {
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
    pub(crate) items: Vec<ItemDef>,
    #[serde(default)]
    pub(crate) pressure_plates: Vec<PressurePlateDef>,
    pub(crate) levels: Vec<LevelDef>,
    #[serde(default)]
    pub(crate) ramps: Vec<RampDef>,
    #[serde(default)]
    pub(crate) ladders: Vec<LadderDef>,
    #[serde(default)]
    pub(crate) moving_floors: Vec<MovingFloorDef>,
}

// Editor-authored moving floor: a tile that slides between the centers of
// two cells, `from` on `level` and `to` on `to_level`, at `speed` meters per
// second, resting `pause_secs` at each end; `phase_secs` offsets its cycle
// so neighbouring tiles need not move in step. Top-level like ramps and
// ladders because it may cross storeys.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MovingFloorDef {
    pub(crate) level: u32,
    pub(crate) from: [i32; 2],
    pub(crate) to: [i32; 2],
    #[serde(default)]
    pub(crate) to_level: Option<u32>,
    pub(crate) speed: f32,
    #[serde(default)]
    pub(crate) pause_secs: f32,
    #[serde(default)]
    pub(crate) phase_secs: f32,
    #[serde(flatten)]
    pub(crate) materials: FaceMaterials,
}

impl MovingFloorDef {
    pub(crate) fn to_level(&self) -> u32 {
        self.to_level.unwrap_or(self.level)
    }

    // The storeys the tile passes through.
    pub(crate) fn swept_levels(&self) -> std::ops::RangeInclusive<u32> {
        self.level.min(self.to_level())..=self.level.max(self.to_level())
    }

    // Whether the tile's body sweeps over `cell` on its way from end to end:
    // the tile is `MOVING_FLOOR_INSET_FRACTION` smaller than a cell on every
    // side, so a cell beside its straight path (the corner of a diagonal's
    // box, or a neighbour it slides past) is untouched.
    pub(crate) fn path_reaches_cell(&self, cell: [i32; 2]) -> bool {
        let [col, row] = cell;
        self.path_reaches_rect([col as f32, row as f32, col as f32 + 1.0, row as f32 + 1.0])
    }

    pub(crate) fn path_reaches_edge(&self, edge: [i32; 4]) -> bool {
        let [c0, r0, c1, r1] = edge;
        self.path_reaches_rect([
            c0.min(c1) as f32,
            r0.min(r1) as f32,
            c0.max(c1) as f32,
            r0.max(r1) as f32,
        ])
    }

    // Grid units: the center path between the two cell centers against the
    // rectangle grown by the tile's half size (Liang–Barsky clipping).
    fn path_reaches_rect(&self, rect: [f32; 4]) -> bool {
        let half = 0.5 - MOVING_FLOOR_INSET_FRACTION;
        let [min_x, min_y, max_x, max_y] = [rect[0] - half, rect[1] - half, rect[2] + half, rect[3] + half];
        let (x0, y0) = (self.from[0] as f32 + 0.5, self.from[1] as f32 + 0.5);
        let (dx, dy) = (
            self.to[0] as f32 - self.from[0] as f32,
            self.to[1] as f32 - self.from[1] as f32,
        );
        let mut t0 = 0.0_f32;
        let mut t1 = 1.0_f32;
        for (p, q) in [(-dx, x0 - min_x), (dx, max_x - x0), (-dy, y0 - min_y), (dy, max_y - y0)] {
            if p == 0.0 {
                if q < 0.0 {
                    return false;
                }
            } else if p < 0.0 {
                t0 = t0.max(q / p);
            } else {
                t1 = t1.min(q / p);
            }
        }
        t0 <= t1
    }
}

// Editor-authored ladder: a `(cell, side)` edge anchor plus how many storeys
// it spans. Top-level (not per-level) because a ladder crosses levels, like
// ramps. The climb volume sits in the adjacent cell across the edge; landings
// are the anchor cell's floors.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub(crate) struct LadderDef {
    pub(crate) lower_level: u32,
    pub(crate) col: i32,
    pub(crate) row: i32,
    pub(crate) side: WallSide,
    #[serde(default = "default_ladder_levels")]
    pub(crate) levels: u32,
}

const fn default_ladder_levels() -> u32 {
    1
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
    pub(crate) grass: Vec<CellDef>,
    #[serde(default)]
    pub(crate) walls: Vec<WallDef>,
    #[serde(default)]
    pub(crate) barriers: Vec<BarrierDef>,
    #[serde(default)]
    pub(crate) light_bridges: Vec<LightBridgeDef>,
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

#[derive(Debug, Deserialize)]
pub(crate) struct FloorDef {
    pub(crate) col: i32,
    pub(crate) row: i32,
    #[serde(flatten)]
    pub(crate) materials: FaceMaterials,
}

// A bare grid cell — grass entries carry no materials.
#[derive(Debug, Deserialize)]
pub(crate) struct CellDef {
    pub(crate) col: i32,
    pub(crate) row: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WallDef {
    pub(crate) c0: i32,
    pub(crate) r0: i32,
    pub(crate) c1: i32,
    pub(crate) r1: i32,
    #[serde(flatten)]
    pub(crate) materials: FaceMaterials,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BarrierDef {
    pub(crate) c0: i32,
    pub(crate) r0: i32,
    pub(crate) c1: i32,
    pub(crate) r1: i32,
    // String id, looked up in the loaded `BarrierKindTable` at compile time.
    pub(crate) kind: String,
}

// One cell of a light bridge. Same-kind cells merge into rectangles at
// compile time (`map::bridges`), so authoring stays per cell like floors.
#[derive(Debug, Deserialize)]
pub(crate) struct LightBridgeDef {
    pub(crate) col: i32,
    pub(crate) row: i32,
    // String id, looked up in the loaded `BridgeKindTable` at compile time.
    pub(crate) kind: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RampDef {
    pub(crate) low: [i32; 2],
    pub(crate) high: [i32; 2],
    pub(crate) lower_level: u32,
    #[serde(flatten)]
    pub(crate) materials: FaceMaterials,
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

// A single map-authored item. `item_type` is an `ItemType` config id
// (`ItemType::from_config_id`), or "key" with `kind` referencing the
// `BarrierKindTable`. Placed items hide on pickup and reappear in place
// after the map's per-type `placed_items.respawn_secs` delay.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct ItemDef {
    pub(crate) level: u32,
    pub(crate) col: i32,
    pub(crate) row: i32,
    #[serde(rename = "type")]
    pub(crate) item_type: String,
    #[serde(default)]
    pub(crate) kind: Option<String>,
}

// A single-cell plate with a purpose (see `pressure_plates_system`): open
// every barrier of a kind, power every light bridge of a kind while enough
// plates of that kind are held, or launch the firework show once enough
// players stand on firework plates.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct PressurePlateDef {
    pub(crate) level: u32,
    pub(crate) col: i32,
    pub(crate) row: i32,
    #[serde(flatten)]
    pub(crate) purpose: PressurePlatePurposeDef,
}

// `{"type": "barrier", "kind": "lobby"}` / `{"type": "bridge", "kind":
// "skyway"}` / `{"type": "firework"}`; `kind` references `BarrierKindTable`
// or `BridgeKindTable` by id, whichever the type names.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum PressurePlatePurposeDef {
    Barrier { kind: String },
    Bridge { kind: String },
    Firework,
}
