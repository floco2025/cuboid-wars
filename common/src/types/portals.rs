use bincode::{Decode, Encode};

use bevy_ecs::prelude::Resource;

use super::{MovingFloorId, PortalPairId, Position};

// Which end of a portal pair. Ends are peers — travel works in both
// directions — the split only gives re-shooting a stable target and the
// client a color per end.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub enum PortalEnd {
    A,
    B,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Resource, Encode, Decode)]
pub enum PortalAccess {
    None,
    Single { pair: PortalPairId, end: PortalEnd },
    Both { pair: PortalPairId },
}

impl PortalAccess {
    #[must_use]
    pub const fn pair(self) -> Option<PortalPairId> {
        match self {
            Self::None => None,
            Self::Single { pair, .. } | Self::Both { pair } => Some(pair),
        }
    }

    #[must_use]
    pub fn allows(self, end: PortalEnd) -> bool {
        match self {
            Self::None => false,
            Self::Single { end: assigned, .. } => assigned == end,
            Self::Both { .. } => true,
        }
    }
}

// One placed portal end: surface geometry plus, for a portal on a moving
// floor, the tile it rides. `pos` is the aperture center — relative to the
// tile's surface center when `anchor` names a tile, world space otherwise —
// so the wire value stays the same while the tile moves and both sides
// place the aperture from their own tile poses. `(nx, ny, nz)` is the
// outward unit normal (tiles only translate, so it never changes). `yaw` is
// the shooter's facing at placement; it orients the aperture frame only
// where world-up is degenerate (near-vertical normals).
#[derive(Debug, Copy, Clone, PartialEq, Encode, Decode)]
pub struct Portal {
    pub pair: PortalPairId,
    pub end: PortalEnd,
    pub pos: Position,
    pub nx: f32,
    pub ny: f32,
    pub nz: f32,
    pub yaw: f32,
    pub anchor: Option<MovingFloorId>,
}
