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
pub use resources::{ActorInfo, ActorMap, ActorSpawnThrottles, ActorSpawner, PendingActorSpawn, PendingActorSpawns};
pub(crate) use resources::{ActorMode, ActorRoute, BeamState};
pub(crate) use spawning::expire_actor_spawn_cooldowns;
pub use spawning::{actors_initial_spawn_system, actors_pending_spawn_system, actors_respawn_system};
