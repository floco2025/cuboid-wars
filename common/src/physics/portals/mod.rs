mod frame;
mod placement;
mod refresh;
mod traversal;

pub use frame::PortalFrame;
pub use placement::{PortalPlacement, PortalPlacementFailure, compute_portal_placement, portal_placement_overlaps};
pub use refresh::carried_portals_refresh_system;
pub use traversal::{
    CharacterHopBody, CharacterPortalHop, PlayerHopBody, PortalSet, ProjectileHop, traverse_move_intent,
    traverse_vector,
};

#[cfg(test)]
mod tests;
