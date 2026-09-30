mod frame;
mod funnel;
mod placement;
mod traversal;

pub use frame::PortalFrame;
pub(crate) use funnel::floor_funnel_prediction;
pub use funnel::{FunnelPrediction, FunnelStep};
pub use placement::{
    PortalPlacement, PortalPlacementFailure, compute_portal_placement, portal_placement_overlaps, portal_placement_yaw,
};
pub use traversal::{
    CharacterHopBody, CharacterPortalHop, PortalSet, ProjectileHop, StraddledGate, in_character_aperture,
    traverse_move_intent, traverse_point, traverse_rotation, traverse_vector, traverse_yaw,
};

#[cfg(test)]
mod tests;
