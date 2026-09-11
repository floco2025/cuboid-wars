use std::collections::HashMap;

use serde::Deserialize;

use common::protocol::{CheckpointKind, FaceMaterials, SwitchDef};

#[derive(Debug, Deserialize)]
pub(crate) struct MapFile {
    pub(crate) map: MapDef,
}

// A loaded root document: its geometry, the named geometry it embeds, and
// the catalogs only the root carries (`load` moves them off the root `MapDef`).
#[derive(Debug)]
pub(crate) struct MapSource {
    pub(crate) geometry: MapDef,
    pub(crate) nested_geometry: HashMap<String, MapDef>,
    pub(crate) switch_kinds: Vec<SwitchDef>,
    pub(crate) fireworks: Option<FireworksConfig>,
}

// The switch that plays the firework show: while it is active a show
// starts, plays, waits `cooldown_secs`, and repeats.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FireworksConfig {
    pub switch: String,
    pub cooldown_secs: f32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MapDef {
    pub(crate) grid_cols: i32,
    pub(crate) grid_rows: i32,
    #[serde(default)]
    pub(crate) actor_spawn_zones: Vec<ActorSpawnZoneDef>,
    #[serde(default)]
    pub(crate) player_spawn_zones: Vec<ZoneDef>,
    #[serde(default)]
    pub(crate) checkpoints: Vec<CheckpointDef>,
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
    pub(crate) nested_maps: Vec<NestedMapDef>,
    // Root only: `load` rejects them on nested geometry.
    #[serde(default)]
    pub(crate) switch_kinds: Vec<SwitchDef>,
    #[serde(default)]
    pub(crate) fireworks: Option<FireworksConfig>,
    #[serde(default)]
    pub(crate) nested_geometry: HashMap<String, MapDef>,
}

// How a nested map moves: between two cells, `from` on `level` and `to` on
// `to_level`, one leg taking `travel_secs`, resting `pause_secs` at each
// end; `phase_secs` offsets its cycle so neighbours need not move in step.
// `from_nudge` and `to_nudge` displace each end from its anchor: x and z
// in wall widths (across columns and rows), y in floor thicknesses (up).
// Two floors meeting at a grid line overlap by one wall width (each
// extends half past its line), so a nudge of one width and a hair back
// along the travel keeps a floor clear of the one it meets. `switch` names
// the map switch that runs the motion; without one it runs from the start.
// Top-level like ramps and ladders because it may cross storeys.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MotionDef {
    pub(crate) level: u32,
    pub(crate) from: [i32; 2],
    pub(crate) to: [i32; 2],
    #[serde(default)]
    pub(crate) to_level: Option<u32>,
    pub(crate) travel_secs: f32,
    #[serde(default)]
    pub(crate) pause_secs: f32,
    #[serde(default)]
    pub(crate) phase_secs: f32,
    #[serde(default)]
    pub(crate) from_nudge: [f32; 3],
    #[serde(default)]
    pub(crate) to_nudge: [f32; 3],
    #[serde(default)]
    pub(crate) switch: Option<String>,
    #[serde(default)]
    pub(crate) switch_inverted: bool,
}

impl MotionDef {
    pub(crate) fn to_level(&self) -> u32 {
        self.to_level.unwrap_or(self.level)
    }
}

// Editor-authored nested map: named geometry placed with its cell (0, 0)
// on the motion's `from` cell, sliding to `to`; a stationary one is a room
// placed once. Its records compile in their own frame under a carrier.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct NestedMapDef {
    pub(crate) map: String,
    #[serde(flatten)]
    pub(crate) motion: MotionDef,
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
    pub(crate) erasers: Vec<EraserDef>,
    #[serde(default)]
    pub(crate) light_bridges: Vec<LightBridgeDef>,
    #[serde(default)]
    pub(crate) lights: Vec<WallLightDef>,
}

// Editor-authored wall light. Identifies a `(cell, side)` pair on this level;
// the runtime resolves its position and facing on the wall.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct WallLightDef {
    pub(crate) kind: String,
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
#[serde(deny_unknown_fields)]
pub(crate) struct BarrierDef {
    pub(crate) c0: i32,
    pub(crate) r0: i32,
    pub(crate) c1: i32,
    pub(crate) r1: i32,
    // String id, looked up in the loaded `BarrierKindTable` at compile time.
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) switch: Option<String>,
    #[serde(default)]
    pub(crate) switch_inverted: bool,
}

// One cell of a light bridge. Same-kind cells merge into rectangles at
// compile time (`map::bridges`), so authoring stays per cell like floors.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LightBridgeDef {
    pub(crate) col: i32,
    pub(crate) row: i32,
    // String id, looked up in the loaded `BridgeKindTable` at compile time.
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) switch: Option<String>,
    #[serde(default)]
    pub(crate) switch_inverted: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RampDef {
    pub(crate) low: [i32; 2],
    pub(crate) high: [i32; 2],
    pub(crate) lower_level: u32,
    #[serde(flatten)]
    pub(crate) materials: FaceMaterials,
}

// `switch` names the map switch that activates the zone; without one the
// zone fills at startup and refills on its kind's timer.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct ActorSpawnZoneDef {
    pub(crate) level: u32,
    pub(crate) cols: [i32; 2],
    pub(crate) rows: [i32; 2],
    pub(crate) kind: String,
    pub(crate) count: u32,
    #[serde(default)]
    pub(crate) switch: Option<String>,
    #[serde(default)]
    pub(crate) switch_inverted: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct ZoneDef {
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

// A single-cell plate operating one of the map's switches by id (see
// `pressure_plates_system`); what the switch drives is declared on its
// targets.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct PressurePlateDef {
    pub(crate) level: u32,
    pub(crate) col: i32,
    pub(crate) row: i32,
    pub(crate) switch: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EraserDef {
    pub(crate) c0: i32,
    pub(crate) r0: i32,
    pub(crate) c1: i32,
    pub(crate) r1: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CheckpointDef {
    #[serde(flatten)]
    pub(crate) zone: ZoneDef,
    #[serde(rename = "type")]
    pub(crate) kind: CheckpointKind,
}
