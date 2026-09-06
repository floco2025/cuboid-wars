use anyhow::Result;
use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};
use serde::Deserialize;

use crate::config::{MapGeometryConfig, MapMovementConfig, PortalShotSettings};

use super::{
    BarrierKindId, BarrierKindTable, BridgeKindId, BridgeKindTable, CarrierId, ItemType, KindDef, Position,
    face_materials::FaceMaterials,
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

#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct WallLight {
    pub pos: Position,
    pub yaw: f32,
    pub carrier: CarrierId,
}

// `levels` counts the storeys spanned: stacked same-kind barriers with no
// floor slab beside the edge between them compile into one record
// (`server/src/map/barriers.rs`).
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Barrier {
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

// A plate-powered walkway: one merged rectangle of same-kind cells, a thin
// slab whose standing surface is `y`. Solid and lit only while its kind is
// powered (`PlateState.powered_bridge_kinds`, applied to the collider by
// `CollisionWorld::set_powered_bridges`).
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct LightBridge {
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
// back, held (`map::carrier_offset_at`, a pure function of the shared tick).
// `level` is the parent storey its local level 0 sits on and `levels` the
// storeys the motion spans, for level focus. Parents precede their children
// in `MapLayout.carriers`. A moving tile is a nested one-cell map.
#[derive(Debug, Clone, Encode, Decode, Copy)]
pub struct Carrier {
    pub parent: CarrierId,
    pub level: u8,
    pub levels: u8,
    pub from: Position,
    pub to: Position,
    pub travel_ticks: u32,
    pub pause_ticks: u32,
    pub phase_ticks: u32,
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

// Visual materials for each segment in the layout. The vectors run parallel
// to `walls` / `ramps` / `floors`: the segment at index `i` renders with the
// `FaceMaterials` at index `i` of the corresponding `*_materials` vector.
// Physics ignores the material vectors.
// What holding a plate does. Barrier plates open every barrier of their kind
// (fully passable + invisible, globally) while enough of them are held —
// distinct from keys (per-player filter). Bridge plates power every light
// bridge of their kind (solid + lit) on the same terms. Firework plates
// launch the show once enough players stand on them. Thresholds live on the
// server; clients receive the held state via `SSnapshot.plates` and the
// show via `SFirework`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Encode, Decode)]
pub enum PlatePurpose {
    Barrier(BarrierKindId),
    Bridge(BridgeKindId),
    Firework,
}

// Coop puzzle primitive: a floor-cell-mounted plate. The center is shipped
// here (not col/row) so the client never needs `MapGeometry` to position
// the visual marker. The server keeps the original (col, row) on its own
// runtime mirror for plate-occupancy tests.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PressurePlate {
    pub level: u8,
    pub center_x: f32,
    pub center_y: f32,
    pub center_z: f32,
    pub purpose: PlatePurpose,
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

#[derive(Debug, Clone, Encode, Decode, Resource, Default)]
pub struct MapLayout {
    pub walls: Vec<Wall>,
    pub wall_materials: Vec<FaceMaterials>,
    pub ramps: Vec<Ramp>,
    pub ramp_materials: Vec<FaceMaterials>,
    pub floors: Vec<Floor>,
    pub floor_materials: Vec<FaceMaterials>,
    pub wall_lights: Vec<WallLight>,
    pub barriers: Vec<Barrier>,
    pub light_bridges: Vec<LightBridge>,
    pub carriers: Vec<Carrier>,
    pub ladders: Vec<Ladder>,
    pub pressure_plates: Vec<PressurePlate>,
    pub grass: Vec<GrassCell>,
}

impl MapLayout {
    // One-line element tally for the server's generate log and the
    // client's spawn log, so both report the same things in the same order.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} walls, {} floors, {} ramps, {} ladders, {} barriers, {} light bridges, {} carriers, {} wall lights, {} pressure plates",
            self.walls.len(),
            self.floors.len(),
            self.ramps.len(),
            self.ladders.len(),
            self.barriers.len(),
            self.light_bridges.len(),
            self.carriers.len(),
            self.wall_lights.len(),
            self.pressure_plates.len(),
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

// Per-map tuning defined in `config/server/gameplay.json` under `maps` and
// shipped to clients in `SInit` so prediction uses the server's values.
#[derive(Debug, Clone, Encode, Decode, Resource, Deserialize)]
pub struct MapSettings {
    pub skybox: String,
    pub geometry: MapGeometryConfig,
    pub movement: MapMovementConfig,
    pub weapons: MapWeaponSettings,
    pub portal_shots: PortalShotSettings,
    // Ordered catalog assigning this map's stable `BarrierKindId` values;
    // empty when the map has no barriers, keys, or barrier plates.
    pub barrier_kinds: Vec<KindDef>,
    // Same for `BridgeKindId`; empty when the map has no light bridges or
    // bridge plates.
    pub bridge_kinds: Vec<KindDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Deserialize)]
pub struct MapWeaponSettings {
    pub projectiles: bool,
    pub missiles: bool,
    pub portals: PortalMode,
}

impl MapWeaponSettings {
    // Whether players can hurt actors here at all; portals are not a weapon.
    #[must_use]
    pub const fn arms_players(self) -> bool {
        self.projectiles || self.missiles
    }

    // A pickup for a disabled weapon never spawns or is granted: its ammo
    // could not be fired here.
    #[must_use]
    pub const fn allows_item(self, item: ItemType) -> bool {
        match item {
            ItemType::MultiShotPowerUp => self.projectiles,
            ItemType::MissilePack => self.missiles,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortalMode {
    None,
    Single,
    Both,
}

impl MapSettings {
    // The id tables both sides build once at startup from the catalogs.
    pub fn kind_tables(&self) -> Result<(BarrierKindTable, BridgeKindTable)> {
        Ok((
            BarrierKindTable::from_defs(&self.barrier_kinds)?,
            BridgeKindTable::from_defs(&self.bridge_kinds)?,
        ))
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
mod tests {
    use super::*;

    fn carrier(parent: CarrierId, level: u8, levels: u8) -> Carrier {
        Carrier {
            parent,
            level,
            levels,
            from: Position::default(),
            to: Position::default(),
            travel_ticks: 1,
            pause_ticks: 0,
            phase_ticks: 0,
        }
    }

    #[test]
    fn carrier_base_level_sums_the_parent_chain() {
        let layout = MapLayout {
            carriers: vec![carrier(CarrierId::WORLD, 2, 0), carrier(CarrierId(1), 1, 0)],
            ..Default::default()
        };
        assert_eq!(layout.carrier_base_level(CarrierId::WORLD), 0);
        assert_eq!(layout.carrier_base_level(CarrierId(1)), 2);
        assert_eq!(layout.carrier_base_level(CarrierId(2)), 3);
    }

    #[test]
    fn carrier_motion_levels_sum_the_parent_chain() {
        let layout = MapLayout {
            carriers: vec![carrier(CarrierId::WORLD, 0, 1), carrier(CarrierId(1), 0, 2)],
            ..Default::default()
        };
        assert_eq!(layout.carrier_motion_levels(CarrierId::WORLD), 0);
        assert_eq!(layout.carrier_motion_levels(CarrierId(1)), 1);
        assert_eq!(layout.carrier_motion_levels(CarrierId(2)), 3);
    }
}
