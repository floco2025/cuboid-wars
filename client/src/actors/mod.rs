mod aim_rig;
#[cfg(test)]
#[path = "tests/aim_rig.rs"]
mod aim_rig_tests;
mod components;
mod interpolation;
mod plugin;
mod resources;
mod spawn;
mod transform_sync;
mod wheel_animation;
#[cfg(test)]
#[path = "tests/wheel_animation.rs"]
mod wheel_animation_tests;
mod wheel_grounding;
#[cfg(test)]
#[path = "tests/wheel_grounding.rs"]
mod wheel_grounding_tests;

pub(crate) use aim_rig::{AimJointMarker, AimRig, FixedFacingMarker};
pub(crate) use components::{ActorAnimationVelocity, RemoteActorMotion};
pub(crate) use interpolation::interpolate_remote_actors_system;
pub use plugin::actor_visuals_plugin;
pub use resources::{ActorGhostMap, ActorInfo, ActorMap};
pub use spawn::{beam_in_ghost_state, spawn_actor, spawn_actor_ghost};
pub(crate) use transform_sync::actors_transform_sync_system;
pub(crate) use wheel_animation::wheel_animation_update_system;
pub(crate) use wheel_grounding::wheel_grounding_system;
