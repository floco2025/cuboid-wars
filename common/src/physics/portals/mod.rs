mod frame;
mod placement;
mod traversal;

pub use frame::PortalFrame;
pub use placement::{PortalPlacement, compute_portal_placement, portal_placement_overlaps};
pub use traversal::{CharacterPortalHop, PortalSet, ProjectileHop, traverse_move_intent, traverse_vector};

#[cfg(test)]
mod tests;
