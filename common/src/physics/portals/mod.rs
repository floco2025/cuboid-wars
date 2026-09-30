mod frame;
mod funnel;
mod placement;
mod traversal;

pub use frame::PortalFrame;
pub(crate) use funnel::FunnelStep;
pub use funnel::{FunnelPrediction, floor_funnel_prediction, funnel_captures};
pub use placement::{
    PortalPlacement, PortalPlacementFailure, compute_portal_placement, portal_placement_overlaps, portal_placement_yaw,
};
pub use traversal::{
    CharacterHopBody, CharacterPortalHop, PlayerHopBody, PortalSet, ProjectileHop, StraddledGate,
    in_character_aperture, traverse_move_intent, traverse_point, traverse_rotation, traverse_vector, traverse_yaw,
};

#[cfg(test)]
mod tests;
