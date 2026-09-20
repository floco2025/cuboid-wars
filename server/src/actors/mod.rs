mod behavior;
mod falling;
mod movement;
pub mod navigation;
mod plugin;
mod removal;
mod resources;
mod spawning;
#[cfg(test)]
pub(crate) mod test_kinds;

pub use behavior::stationary_actors_behavior_system;
pub use falling::actors_fall_damage_system;
pub use movement::SurfaceAgent;
pub use movement::traversal::{TraversalAction, TraversalEnvironment, TraversalExecutor, TraversalStatus};
pub(crate) use movement::{ActorMovementQuery, apply_actor_moves, plan_actor_moves};
pub(crate) use movement::{ActorMovementStep, step_actor_movement};
pub(crate) use movement::{SurfaceGoal, surface_actors_movement_system};
pub use plugin::actors_plugin;
pub use removal::actors_removal_system;
pub use resources::{
    ActorCharacter, ActorCrushed, ActorInfo, ActorLanding, ActorMap, ActorMotionQuery, ActorSpawner, ActorStateQuery,
    PendingActorSpawn, PendingActorSpawns,
};
pub(crate) use resources::{ActorMode, BeamState};
pub use spawning::{actors_pending_spawn_system, actors_respawn_system, pending_actor_spawns_active};
pub(crate) use spawning::{expedite_actor_respawns, reset_actors};
