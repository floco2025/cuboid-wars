use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::{Value, from_value};

use common::protocol::{CarrierMotion, CheckpointKind, FaceMaterials, KindDef, SwitchDef, TERRAIN_MATERIAL};

use crate::CheckpointResponse;
use common::config::deserialize_required_option;

#[derive(Debug, Deserialize, Serialize)]
pub struct MapFile {
    #[serde(deserialize_with = "deserialize_root_map")]
    pub map: MapDef,
}

fn deserialize_root_map<'de, D>(deserializer: D) -> Result<MapDef, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    if !value.as_object().is_some_and(|map| map.contains_key("fireworks")) {
        return Err(de::Error::missing_field("fireworks"));
    }
    from_value(value).map_err(de::Error::custom)
}

// A loaded root document: its geometry, the named geometry it embeds, and
// the catalogs only the root carries (`load` moves them off the root `MapDef`).
#[derive(Debug)]
pub struct MapSource {
    pub geometry: MapDef,
    pub nested_geometry: HashMap<String, MapDef>,
    pub switches: Vec<SwitchDef>,
    pub barrier_kinds: Vec<KindDef>,
    pub bridge_kinds: Vec<KindDef>,
    pub fireworks: Option<FireworksConfig>,
}

// The switch that plays the firework show: while it is active a show
// starts, plays, waits `cooldown_secs`, and repeats.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FireworksConfig {
    pub switch: String,
    #[serde(serialize_with = "crate::values::serialize_number")]
    pub cooldown_secs: f32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapDef {
    pub grid_cols: i32,
    pub grid_rows: i32,
    #[serde(default)]
    pub actor_spawn_zones: Vec<ActorSpawnZoneDef>,
    #[serde(default)]
    pub checkpoints: Vec<CheckpointDef>,
    #[serde(default)]
    pub items: Vec<ItemDef>,
    #[serde(default)]
    pub pressure_plates: Vec<PressurePlateDef>,
    pub levels: Vec<LevelDef>,
    #[serde(default)]
    pub ramps: Vec<RampDef>,
    #[serde(default)]
    pub ladders: Vec<LadderDef>,
    #[serde(default)]
    pub nested_maps: Vec<NestedMapDef>,
    // Root only: `load` rejects them on nested geometry.
    #[serde(default)]
    pub switches: Vec<SwitchDef>,
    #[serde(default)]
    pub barrier_kinds: Vec<KindDef>,
    #[serde(default)]
    pub bridge_kinds: Vec<KindDef>,
    #[serde(default)]
    pub fireworks: Option<FireworksConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub nested_geometry: HashMap<String, MapDef>,
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
// `motion` defaults to Cycle. FollowSwitch requires a switch and targets
// end 2 while its response matches, end 1 otherwise, ignoring cycle timing.
// Top-level like ramps and ladders because it may cross storeys.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MotionDef {
    pub level: u32,
    pub from: [i32; 2],
    pub to: [i32; 2],
    #[serde(default)]
    pub to_level: Option<u32>,
    #[serde(serialize_with = "crate::values::serialize_number")]
    pub travel_secs: f32,
    #[serde(default)]
    #[serde(serialize_with = "crate::values::serialize_number")]
    pub pause_secs: f32,
    #[serde(default)]
    #[serde(serialize_with = "crate::values::serialize_number")]
    pub phase_secs: f32,
    #[serde(default)]
    #[serde(serialize_with = "crate::values::serialize_nudge")]
    pub from_nudge: [f32; 3],
    #[serde(default)]
    #[serde(serialize_with = "crate::values::serialize_nudge")]
    pub to_nudge: [f32; 3],
    #[serde(default)]
    pub switch: Option<String>,
    #[serde(default)]
    pub switch_inverted: bool,
    #[serde(default)]
    pub motion: CarrierMotion,
}

impl MotionDef {
    pub fn to_level(&self) -> u32 {
        self.to_level.unwrap_or(self.level)
    }
}

// Editor-authored nested map: named geometry placed with its cell (0, 0)
// on the motion's `from` cell, sliding to `to`; a stationary one is a room
// placed once. Its records compile in their own frame under a carrier.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NestedMapDef {
    pub map: String,
    #[serde(flatten)]
    pub motion: MotionDef,
}

// Editor-authored ladder: a `(cell, side)` edge anchor plus how many storeys
// it spans. Top-level (not per-level) because a ladder crosses levels, like
// ramps. The climb volume sits in the adjacent cell across the edge; landings
// are the anchor cell's floors.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
pub struct LadderDef {
    pub lower_level: u32,
    pub col: i32,
    pub row: i32,
    pub side: WallSide,
    #[serde(default = "default_ladder_levels")]
    pub levels: u32,
}

const fn default_ladder_levels() -> u32 {
    1
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LevelDef {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub floors: Vec<FloorDef>,
    #[serde(default)]
    pub inaccessible_floors: Vec<FloorDef>,
    #[serde(default)]
    pub terrain: Vec<TerrainDef>,
    #[serde(default)]
    pub walls: Vec<WallDef>,
    #[serde(default)]
    pub barriers: Vec<BarrierDef>,
    #[serde(default)]
    pub erasers: Vec<EraserDef>,
    #[serde(default)]
    pub light_bridges: Vec<LightBridgeDef>,
    #[serde(default)]
    pub lights: Vec<WallLightDef>,
}

// Editor-authored wall light. Identifies a `(cell, side)` pair on this level;
// the runtime resolves its position and facing on the wall.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct WallLightDef {
    pub kind: String,
    pub col: i32,
    pub row: i32,
    pub side: WallSide,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
pub enum WallSide {
    #[serde(rename = "N")]
    North,
    #[serde(rename = "S")]
    South,
    #[serde(rename = "E")]
    East,
    #[serde(rename = "W")]
    West,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FloorDef {
    pub col: i32,
    pub row: i32,
    #[serde(flatten)]
    pub materials: FaceMaterials,
}

// Terrain is a floor slab whose top always uses the procedural terrain
// material. Authors provide its bottom and four side materials, with `all`
// as the usual shorthand.
#[derive(Debug)]
pub struct TerrainDef {
    pub col: i32,
    pub row: i32,
    pub materials: FaceMaterials,
}

impl Serialize for TerrainDef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(7))?;
        map.serialize_entry("col", &self.col)?;
        map.serialize_entry("row", &self.row)?;
        for (face, material) in [
            ("bottom", &self.materials.bottom),
            ("north", &self.materials.north),
            ("south", &self.materials.south),
            ("east", &self.materials.east),
            ("west", &self.materials.west),
        ] {
            map.serialize_entry(face, material)?;
        }
        map.end()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TerrainDefWire {
    col: i32,
    row: i32,
    #[serde(default)]
    all: Option<String>,
    #[serde(default)]
    bottom: Option<String>,
    #[serde(default)]
    north: Option<String>,
    #[serde(default)]
    south: Option<String>,
    #[serde(default)]
    east: Option<String>,
    #[serde(default)]
    west: Option<String>,
}

impl<'de> Deserialize<'de> for TerrainDef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TerrainDefWire::deserialize(deserializer)?;
        let pick = |face: Option<String>| -> Result<String, D::Error> {
            face.or_else(|| wire.all.clone()).ok_or_else(|| {
                de::Error::custom("missing terrain material; provide `all` or bottom/north/south/east/west")
            })
        };
        Ok(Self {
            col: wire.col,
            row: wire.row,
            materials: FaceMaterials {
                top: TERRAIN_MATERIAL.to_owned(),
                bottom: pick(wire.bottom)?,
                north: pick(wire.north)?,
                south: pick(wire.south)?,
                east: pick(wire.east)?,
                west: pick(wire.west)?,
            },
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WallDef {
    pub c0: i32,
    pub r0: i32,
    pub c1: i32,
    pub r1: i32,
    #[serde(flatten)]
    pub materials: FaceMaterials,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BarrierDef {
    pub c0: i32,
    pub r0: i32,
    pub c1: i32,
    pub r1: i32,
    // String id, looked up in the loaded `BarrierKindTable` at compile time.
    pub kind: String,
    #[serde(default)]
    pub switch: Option<String>,
    #[serde(default)]
    pub switch_inverted: bool,
}

// One cell of a light bridge. Same-kind cells merge into rectangles at
// compile time (`map::bridges`), so authoring stays per cell like floors.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LightBridgeDef {
    pub col: i32,
    pub row: i32,
    // String id, looked up in the loaded `BridgeKindTable` at compile time.
    pub kind: String,
    #[serde(default)]
    pub switch: Option<String>,
    #[serde(default)]
    pub switch_inverted: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RampDef {
    pub low: [i32; 2],
    pub high: [i32; 2],
    pub lower_level: u32,
    #[serde(flatten)]
    pub materials: FaceMaterials,
}

// `respawn_secs` is the delay before a killed actor's slot refills; `null`
// never refills. `switch` names the map switch that lets the zone spawn;
// without one the zone fills at startup and refills on its timer.
// `until_checkpoint` ends the zone once any player has reached that
// checkpoint, and `on_checkpoint` says whether its remaining actors go too.
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct ActorSpawnZoneDef {
    pub level: u32,
    #[serde(default = "default_zone_levels")]
    pub levels: u32,
    #[serde(default)]
    #[serde(serialize_with = "crate::values::serialize_number")]
    pub roam_distance: f32,
    pub cols: [i32; 2],
    pub rows: [i32; 2],
    pub kind: String,
    pub count: Vec<u32>,
    #[serde(deserialize_with = "deserialize_required_option")]
    #[serde(serialize_with = "crate::values::serialize_optional_number")]
    pub respawn_secs: Option<f32>,
    #[serde(default)]
    pub switch: Option<String>,
    #[serde(default)]
    pub switch_inverted: bool,
    #[serde(default)]
    pub until_checkpoint: Option<u32>,
    #[serde(default)]
    pub on_checkpoint: Option<CheckpointResponse>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct ZoneDef {
    pub level: u32,
    pub cols: [i32; 2],
    pub rows: [i32; 2],
}

const fn default_zone_levels() -> u32 {
    1
}

// A single map-authored item. `item_type` is an `ItemType` config id
// (`ItemType::from_config_id`), or "key" with `kind` referencing the
// `BarrierKindTable`. Placed items hide on pickup and reappear in place
// after the map's per-type `placed_items.respawn_secs` delay, if configured.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct ItemDef {
    pub level: u32,
    pub col: i32,
    pub row: i32,
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(default)]
    pub kind: Option<String>,
}

// A single-cell plate operating one of the map's switches by id (see
// `pressure_plates_system`); what the switch drives is declared on its
// targets.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct PressurePlateDef {
    pub level: u32,
    pub col: i32,
    pub row: i32,
    pub switch: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EraserDef {
    pub c0: i32,
    pub r0: i32,
    pub c1: i32,
    pub r1: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckpointDef {
    #[serde(flatten)]
    pub zone: ZoneDef,
    #[serde(rename = "type")]
    pub kind: CheckpointKind,
    pub number: u32,
}
