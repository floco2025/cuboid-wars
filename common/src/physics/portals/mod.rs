mod frame;
mod placement;
mod traversal;

pub use frame::PortalFrame;
pub use placement::{PortalPlacement, PortalPlacementFailure, compute_portal_placement, portal_placement_overlaps};
pub use traversal::{
    CharacterHopBody, CharacterPortalHop, PlayerHopBody, PortalSet, ProjectileHop, traverse_move_intent,
    traverse_point, traverse_rotation, traverse_vector, traverse_yaw,
};

#[cfg(test)]
mod tests;
