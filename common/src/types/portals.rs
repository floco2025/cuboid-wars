use bincode::{Decode, Encode};

use super::{PlayerId, Position};

// Which end of a player's portal pair. Ends are peers — travel works in both
// directions — the split only gives re-shooting a stable target and the
// client a color per end.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub enum PortalEnd {
    A,
    B,
}

// One placed portal end: pure surface geometry. `pos` is the aperture center
// on the hit surface and `(nx, ny, nz)` its outward unit normal — nothing
// records what kind of surface was hit. `yaw` is the shooter's facing at
// placement; it orients the aperture frame only where world-up is degenerate
// (near-vertical normals).
#[derive(Debug, Copy, Clone, PartialEq, Encode, Decode)]
pub struct Portal {
    pub owner: PlayerId,
    pub end: PortalEnd,
    pub pos: Position,
    pub nx: f32,
    pub ny: f32,
    pub nz: f32,
    pub yaw: f32,
}
