mod actions;
mod baking;
mod contours;
mod corridor;
mod ladders;
mod mesh;
mod regions;
mod route;
mod transfers;
mod world;

pub use actions::{CarrierDock, TraversalAction};
pub use mesh::{SurfaceBounds, SurfaceLocation, SurfaceMesh};
pub(crate) use route::ROUTE_SEARCH_VISITS;
pub use route::{RouteFailure, SurfaceRoute};
pub(crate) use world::{SurfaceNavigation, surface_navigation_sync_system};

#[cfg(test)]
#[path = "tests/fixtures.rs"]
pub(crate) mod fixtures;
