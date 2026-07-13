mod behavior;
mod movement;
pub mod navigation;
mod removal;
mod resources;
mod spawning;

pub use behavior::actor_behavior_system;
pub(crate) use movement::{ActorMovementQuery, apply_actor_moves, plan_actor_moves};
pub use removal::actor_removal_system;
pub use resources::{
    ActorGoal, ActorInfo, ActorMap, ActorSpawnThrottles, ActorSpawner, PendingActorSpawn, PendingActorSpawns,
};
pub use spawning::{actor_initial_spawn_system, actor_pending_spawn_system, actor_respawn_system};
