use std::collections::BTreeMap;

use anyhow::Result;
use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::{
    celestial::CelestialMapSettings,
    config::{MapGeometryConfig, MapMovementConfig, deserialize_required_option},
    map::{Grounds, GroundsSettings},
};

use super::{
    BarrierId, BridgeId, CarrierId, FieldKindId, FieldKindTable, ItemType, KindDef, Position, SwitchDef, SwitchId,
    face_materials::FaceMaterials, textures::TextureSettings,
};

// Layout records are in their carrier's frame: world space for
// `CarrierId::WORLD`, the map itself, and a carrier's local frame otherwise,
// placed by the carrier's pose. `y` is the base surface and `height` the
// rise, both filled by the server from the map's geometry, so neither the
// physics nor the client needs the map's sizes to place them. `level` is the
// storey tag the client's level focus filters on, counted from the carrier's
// own level 0.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Wall {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub width: f32,
    pub y: f32,
    pub height: f32,
    pub level: u8,
    pub carrier: CarrierId,
}

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Floor {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub y: f32,
    pub thickness: f32,
    pub level: u8,
    pub carrier: CarrierId,
}

impl Floor {
    #[must_use]
    pub const fn bounds_xz(&self) -> (f32, f32, f32, f32) {
        (
            self.x1.min(self.x2),
            self.x1.max(self.x2),
            self.z1.min(self.z2),
            self.z1.max(self.z2),
        )
    }
}

// The way a ramp rises: north is -Z, south +Z, west -X, east +X.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Deserialize, Serialize)]
pub enum RampDirection {
    #[serde(rename = "N")]
    North,
    #[serde(rename = "S")]
    South,
    #[serde(rename = "E")]
    East,
    #[serde(rename = "W")]
    West,
}

impl RampDirection {
    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::South => Self::North,
            Self::East => Self::West,
            Self::West => Self::East,
        }
    }
}

// A solid ramp is a wedge filled down to its low edge's height; a plank is a
// slab of `Ramp::thickness` under the slope, open beneath.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Encode, Decode, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RampShape {
    #[default]
    Solid,
    Plank,
}

// `y` is the surface along the low edge and `height` the rise to the edge
// `direction` names, over the whole of `x1..z2`: a solid's cells, and a
// plank's cells plus the overhang a floor slab would have beside the run.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Ramp {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub y: f32,
    pub height: f32,
    pub direction: RampDirection,
    pub shape: RampShape,
    pub thickness: f32,
    pub level: u8,
    pub levels: u8,
    pub carrier: CarrierId,
}

impl Ramp {
    #[must_use]
    pub const fn bounds_xz(&self) -> (f32, f32, f32, f32) {
        (
            self.x1.min(self.x2),
            self.x1.max(self.x2),
            self.z1.min(self.z2),
            self.z1.max(self.z2),
        )
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct WallLight {
    pub kind: String,
    pub pos: Position,
    pub yaw: f32,
    pub carrier: CarrierId,
}

// `levels` counts the storeys spanned: stacked same-kind barriers with no
// floor slab beside the edge between them compile into one record
// (`server/src/map/barriers.rs`). `initially_on` is its state before any
// switch input: solid when set, and its switch flips it while active.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Barrier {
    pub id: BarrierId,
    pub switch: Option<SwitchId>,
    pub initially_on: bool,

    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub width: f32,
    pub y: f32,
    pub height: f32,
    pub level: u8,
    pub levels: u8,
    pub kind: FieldKindId,
    pub carrier: CarrierId,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct Eraser {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub width: f32,
    pub y: f32,
    pub height: f32,
    pub level: u8,
    pub carrier: CarrierId,
}

// A barrier laid flat: one merged rectangle of same-kind cells, a thin slab
// whose standing surface is `y`. Solid and lit while it is on, a ghost while
// it is off (`SwitchState.open_fields`); `initially_on` and `switch` read as
// on a barrier.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct LightBridge {
    pub id: BridgeId,
    pub switch: Option<SwitchId>,
    pub initially_on: bool,
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub y: f32,
    pub thickness: f32,
    pub level: u8,
    pub kind: FieldKindId,
    pub carrier: CarrierId,
}

impl LightBridge {
    #[must_use]
    pub const fn bounds_xz(&self) -> (f32, f32, f32, f32) {
        (
            self.x1.min(self.x2),
            self.x1.max(self.x2),
            self.z1.min(self.z2),
            self.z1.max(self.z2),
        )
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Encode, Decode, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CarrierMotion {
    #[default]
    Cycle,
    FollowSwitch,
}

// A rigid group of map records that slides between two poses. Every record
// naming this carrier is in its local frame; the carrier's origin sits at
// `from` in its parent's frame at end 1 and at `to` at end 2.
// `map::CarrierRun` owns Cycle and FollowSwitch timing, replicated in
// `SwitchState.carrier_runs`; free cycles use the shared tick. `initially_on`
// is its state before any switch input, which its switch flips while
// active: a Cycle runs while on, a FollowSwitch heads for end 2.
// `level` is the parent storey its local level 0 sits on and `levels` the
// storeys the motion spans, for level focus. Parents precede their children
// in `MapLayout.carriers`. A moving tile is a nested one-cell map.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Carrier {
    pub initially_on: bool,
    pub motion: CarrierMotion,
    pub parent: CarrierId,
    pub level: u8,
    pub levels: u8,
    pub from: Position,
    pub to: Position,
    pub travel_ticks: u32,
    pub pause_ticks: u32,
    pub phase_ticks: u32,
    pub switch: Option<SwitchId>,
}

// Freestanding climbable element anchored on a grid edge. The segment is the
// edge span already shrunk to `LADDER_WIDTH` centered on the edge midpoint.
// One-sided: the normal points at the FRONT — the climbable rail side —
// while the back is passed through. No Rapier collider — the character step
// queries the derived climb volumes directly.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Ladder {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub nx: f32,
    pub nz: f32,
    // Base landing surface and the rise to the top landing.
    pub y: f32,
    pub height: f32,
    pub level: u8,
    pub levels: u8,
    pub carrier: CarrierId,
}

// Coop puzzle primitive: a floor-cell-mounted plate that operates one of
// the map's switches (`MapSettings.switches`); what that switch drives is
// declared on its targets. The center and side length are shipped here so
// rendering and collision share the footprint without `MapGeometry`. The
// server keeps the original (col, row) on its own runtime mirror for
// plate-occupancy tests. Clients receive what the switches hold via
// `SSnapshot.switch_state`.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PressurePlate {
    pub level: u8,
    pub center_x: f32,
    pub center_y: f32,
    pub center_z: f32,
    pub side: f32,
    pub switch: SwitchId,
    pub carrier: CarrierId,
}

// The alias of procedural terrain tops and the exterior grounds, bound by
// `assets.json::terrain` rather than a map's textures. Terrain's authored
// bottom and side materials remain ordinary map aliases.
pub const TERRAIN_MATERIAL: &str = "terrain";

// Client-display-only metadata identifying the authored terrain cells. The
// compiled floor slab carries collision and its five authored non-top
// materials; its top material tells the client to render and grass the final
// slab footprint procedurally, including trim. The cell center + floor-top y
// are shipped (not col/row), independently of `MapGeometry`.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct TerrainCell {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub level: u8,
    pub carrier: CarrierId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointKind {
    Individual,
    GroupAny,
    GroupAll,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct Checkpoint {
    // Meaningless on the start, which no entry ever saves.
    pub kind: CheckpointKind,
    // The course position; `--checkpoint`, `/checkpoint`, and a zone's
    // `until_checkpoint` name it. Checkpoints sharing a number are one
    // respawn point with several spots. 0 is the start: every player begins
    // there, and it has no flag or paint.
    pub number: u32,
    pub carrier: CarrierId,
    pub level: u8,
    // The rectangle's cells in the carrier's grid, for the spawn sampler.
    pub cols: [i32; 2],
    pub rows: [i32; 2],
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Encode, Decode, Resource, Default)]
pub struct MapLayout {
    pub grounds: Option<Grounds>,
    pub walls: Vec<Wall>,
    // Visual materials for each segment: the `*_materials` vectors run
    // parallel to `walls` / `ramps` / `floors`, so the segment at index `i`
    // renders with the `FaceMaterials` at index `i`. Portal placement
    // resolves these aliases against the map texture catalog.
    pub wall_materials: Vec<FaceMaterials>,
    pub ramps: Vec<Ramp>,
    pub ramp_materials: Vec<FaceMaterials>,
    pub floors: Vec<Floor>,
    pub floor_materials: Vec<FaceMaterials>,
    pub wall_lights: Vec<WallLight>,
    pub barriers: Vec<Barrier>,
    pub erasers: Vec<Eraser>,
    pub light_bridges: Vec<LightBridge>,
    pub carriers: Vec<Carrier>,
    pub ladders: Vec<Ladder>,
    pub pressure_plates: Vec<PressurePlate>,
    pub terrain: Vec<TerrainCell>,
    pub checkpoints: Vec<Checkpoint>,
}

impl MapLayout {
    // One-line element tally for the server's generate log and the
    // client's spawn log, so both report the same things in the same order.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} walls, {} floors, {} ramps, {} ladders, {} barriers, {} erasers, {} light bridges, {} carriers, {} wall lights, {} pressure plates, {} checkpoints",
            self.walls.len(),
            self.floors.len(),
            self.ramps.len(),
            self.ladders.len(),
            self.barriers.len(),
            self.erasers.len(),
            self.light_bridges.len(),
            self.carriers.len(),
            self.wall_lights.len(),
            self.pressure_plates.len(),
            self.checkpoints.len(),
        )
    }

    #[must_use]
    pub fn carrier(&self, id: CarrierId) -> Option<&Carrier> {
        self.carriers.get(id.carried_index()?)
    }

    // The world storey a carrier's local level 0 sits on: its own placement
    // plus every ancestor's.
    #[must_use]
    pub fn carrier_base_level(&self, id: CarrierId) -> u8 {
        let mut base = 0u8;
        let mut current = self.carrier(id);
        while let Some(carrier) = current {
            base = base.saturating_add(carrier.level);
            current = self.carrier(carrier.parent);
        }
        base
    }

    // How many storeys above its base a carrier's records may reach through
    // its own motion and every ancestor's.
    #[must_use]
    pub fn carrier_motion_levels(&self, id: CarrierId) -> u8 {
        let mut span = 0u8;
        let mut current = self.carrier(id);
        while let Some(carrier) = current {
            span = span.saturating_add(carrier.levels);
            current = self.carrier(carrier.parent);
        }
        span
    }
}

// The map-facing part of a map's effective configuration (the `gameplay.json`
// defaults with the map's `settings.json` overrides), plus the catalogs the
// root layout defines, shipped to clients in `SInit` so prediction uses the
// server's values.
#[derive(Debug, Clone, Encode, Decode, Resource, Deserialize)]
pub struct MapSettings {
    #[serde(deserialize_with = "deserialize_required_option")]
    pub grounds: Option<GroundsSettings>,
    pub celestial: CelestialMapSettings,
    pub textures: BTreeMap<String, TextureSettings>,
    pub geometry: MapGeometryConfig,
    pub movement: MapMovementConfig,
    pub portals: PortalMode,

    // The root layout's ordered catalogs, filled by map generation rather
    // than read from settings.json: `switches` assigns this map's stable
    // `SwitchId` values, each with its activation policy; `field_kinds` its
    // `FieldKindId` values, shared by barriers, light bridges, and keys.
    // Each is empty when the map has none.
    #[serde(skip)]
    pub switches: Vec<SwitchDef>,
    #[serde(skip)]
    pub field_kinds: Vec<KindDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortalMode {
    Single,
    Both,
}

#[derive(Debug, Clone, Default, Encode, Decode, Resource)]
pub struct MapItems(pub Vec<ItemType>);

impl MapItems {
    #[must_use]
    pub fn contains(&self, item: ItemType) -> bool {
        self.0.contains(&item)
    }

    #[must_use]
    pub fn key_kinds(&self) -> Vec<FieldKindId> {
        let mut kinds: Vec<_> = self
            .0
            .iter()
            .filter_map(|item| match item {
                ItemType::Key(kind) => Some(*kind),
                _ => None,
            })
            .collect();
        kinds.sort_unstable();
        kinds.dedup();
        kinds
    }
}

impl MapSettings {
    pub fn field_kind_table(&self) -> Result<FieldKindTable> {
        FieldKindTable::from_defs(&self.field_kinds)
    }

    #[must_use]
    pub fn gravity_for(&self, has_low_gravity: bool) -> f32 {
        if has_low_gravity {
            self.movement.low_gravity
        } else {
            self.movement.gravity
        }
    }
}

#[cfg(test)]
#[path = "tests/map_layout.rs"]
mod tests;
