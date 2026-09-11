use std::collections::BTreeMap;

use anyhow::Result;
use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};
use serde::Deserialize;

use crate::config::{MapGeometryConfig, MapMovementConfig};

use super::{
    BarrierId, BarrierKindId, BarrierKindTable, BridgeId, BridgeKindId, BridgeKindTable, CarrierId, ItemType, KindDef,
    Position, SwitchDef, SwitchId, SwitchTable, face_materials::FaceMaterials, textures::TextureSettings,
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

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Ramp {
    pub x1: f32,
    pub y1: f32,
    pub z1: f32,
    pub x2: f32,
    pub y2: f32,
    pub z2: f32,
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

    #[must_use]
    pub const fn bounds_y(&self) -> (f32, f32) {
        (self.y1.min(self.y2), self.y1.max(self.y2))
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
// (`server/src/map/barriers.rs`).
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Barrier {
    pub id: BarrierId,
    pub switch: Option<SwitchId>,
    pub switch_inverted: bool,

    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub width: f32,
    pub y: f32,
    pub height: f32,
    pub level: u8,
    pub levels: u8,
    pub kind: BarrierKindId,
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

// A plate-powered walkway: one merged rectangle of same-kind cells, a thin
// slab whose standing surface is `y`. Solid and lit only while its instance is
// powered (`PlateState.powered_bridges`, applied to the collider by
// `CollisionWorld::set_powered_bridges`).
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct LightBridge {
    pub id: BridgeId,
    pub switch: Option<SwitchId>,
    pub switch_inverted: bool,
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub y: f32,
    pub thickness: f32,
    pub level: u8,
    pub kind: BridgeKindId,
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

// A rigid group of map records that slides between two poses. Every record
// naming this carrier is in its local frame; the carrier's origin sits at
// `from` in its parent's frame at end 1 and at `to` at end 2, out, held,
// back, held (`map::carrier_offset_at`, a pure function of its run ticks:
// the shared tick for a free carrier, the ticks its switch has kept it
// running for a switched one, replicated in `PlateState.carrier_runs`).
// `level` is the parent storey its local level 0 sits on and `levels` the
// storeys the motion spans, for level focus. Parents precede their children
// in `MapLayout.carriers`. A moving tile is a nested one-cell map.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Carrier {
    pub switch_inverted: bool,
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
// declared on its targets. The center is shipped here (not col/row) so the
// client never needs `MapGeometry` to position the visual marker. The
// server keeps the original (col, row) on its own runtime mirror for
// plate-occupancy tests. Clients receive what the switches hold via
// `SSnapshot.plates`.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PressurePlate {
    pub level: u8,
    pub center_x: f32,
    pub center_y: f32,
    pub center_z: f32,
    pub switch: SwitchId,
    pub carrier: CarrierId,
}

// Client-display-only decoration; physics and gameplay ignore it. The cell
// center + floor-top y are shipped (not col/row) so the client never needs
// `MapGeometry` to scatter tufts.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct GrassCell {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub level: u8,
    pub carrier: CarrierId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointKind {
    Individual,
    GroupAny,
    GroupAll,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct Checkpoint {
    pub kind: CheckpointKind,
    pub carrier: CarrierId,
    pub level: u8,
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Encode, Decode, Resource, Default)]
pub struct MapLayout {
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
    pub grass: Vec<GrassCell>,
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

// Per-map tuning defined in `config/server/maps/<name>/settings.json` and
// shipped to clients in `SInit` so prediction uses the server's values.
#[derive(Debug, Clone, Encode, Decode, Resource, Deserialize)]
pub struct MapSettings {
    pub skybox: String,
    pub textures: BTreeMap<String, TextureSettings>,
    pub geometry: MapGeometryConfig,
    pub movement: MapMovementConfig,
    pub portals: PortalMode,

    // Ordered catalog assigning this map's stable `SwitchId` values, each
    // with its plates' policy; empty when the map has no pressure plates.
    #[serde(skip)]
    pub switches: Vec<SwitchDef>,
    // Ordered catalog assigning this map's stable `BarrierKindId` values;
    // empty when the map has no barriers or keys.
    pub barrier_kinds: Vec<KindDef>,
    // Same for `BridgeKindId`; empty when the map has no light bridges.
    pub bridge_kinds: Vec<KindDef>,
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
    pub fn key_kinds(&self) -> Vec<BarrierKindId> {
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
    pub fn kind_tables(&self) -> Result<(BarrierKindTable, BridgeKindTable, SwitchTable)> {
        let switches = SwitchTable::from_switch_defs(&self.switches)?;
        let barriers = BarrierKindTable::from_defs(&self.barrier_kinds)?;
        let bridges = BridgeKindTable::from_defs(&self.bridge_kinds)?;
        Ok((barriers, bridges, switches))
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
