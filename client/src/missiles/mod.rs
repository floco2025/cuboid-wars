mod air_graph;
mod blast;
mod flight;
mod guidance;
mod interpolation;
mod launch;
mod lock_on;
mod pathfind;
mod steering;

mod movement;
mod resources;
mod spawn;
mod transform_sync;

pub use lock_on::lock_on_system;
pub use movement::missiles_movement_system;
pub use resources::{LockOnTarget, MissileMap, MissileVelocity};
pub use spawn::{MissileAssets, missile_rotation, spawn_missile, spawn_missile_meshes, spawn_missile_pickup_visual};
pub(crate) use transform_sync::missiles_transform_sync_system;

pub(crate) use air_graph::AirGraph;
pub(crate) use flight::MissileFlight;
pub(crate) use guidance::guide_missile;
pub(crate) use launch::clear_launch_direction;

pub(crate) use interpolation::RemoteMissileMotion;
pub(crate) use resources::{MissileInfo, OwnedMissile};

#[cfg(test)]
mod interpolation_tests;
#[cfg(test)]
mod movement_tests;
