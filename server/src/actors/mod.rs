mod behavior;
mod movement;
pub mod navigation;
mod plugin;
mod removal;
mod resources;
mod spawning;

pub use behavior::actors_behavior_system;
pub(crate) use movement::{ActorMovementQuery, apply_actor_moves, plan_actor_moves};
pub use plugin::actors_plugin;
pub use removal::actors_removal_system;
pub use resources::{
    ActorGoal, ActorInfo, ActorMap, ActorSpawnThrottles, ActorSpawner, PendingActorSpawn, PendingActorSpawns,
};
pub use spawning::{actors_initial_spawn_system, actors_pending_spawn_system, actors_respawn_system};
