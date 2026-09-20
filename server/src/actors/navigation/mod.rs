mod escape;
mod frontier;
mod home;
pub(crate) use escape::{evade_clearance, segment_threat_distance_sq};
pub use home::ActorTerritories;
pub(crate) use home::{ActorTerritory, radical_inverse};
pub(crate) mod air;
pub mod surface;
