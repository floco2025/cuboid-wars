use bevy::prelude::*;

mod cuboid_mesh;
mod floors;
mod geometry_batch;
mod ramp_mesh;
mod ramps;
mod walls;

pub use floors::batch_floor;
pub use geometry_batch::MapGeometryBatch;
pub use ramps::batch_ramp;
pub use walls::batch_wall;

// Marker for wall mesh entities.
#[derive(Component)]
pub struct WallMarker;

// Marker for upper-level floor slabs. Level-0 ground is not tagged.
#[derive(Component)]
pub struct RoofMarker;

// Marker for the ground plane (level-0 floor).
#[derive(Component)]
pub struct GroundMarker;

// Marker for ramp mesh entities.
#[derive(Component)]
pub struct RampMarker;

// Records which level a map entity belongs to. Walls and floors get the
// level they sit on; ramps get the lower of the two levels they connect.
// Used by the level-focus toggle to hide entities not at the local
// player's current level.
#[derive(Component, Copy, Clone, Debug)]
pub struct MapLevel(pub u8);
