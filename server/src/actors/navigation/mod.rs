mod escape;
mod frontier;
mod home;
pub(crate) use escape::{evade_clearance, segment_threat_distance_sq};
pub use home::ActorTerritories;
pub(crate) use home::ActorTerritory;
pub(crate) mod air;
mod ground;
pub(crate) use ground::{GroundNavigation, GroundSearchResult, GroundState, GroundTask};

pub use ground::{NavGraph, NavGraphs, nav_bridges_sync_system};
pub(crate) use ground::{NavNode, NavWaypoint, PlannedRoute, WALK_REACH_DISTANCE, WaypointKind};
