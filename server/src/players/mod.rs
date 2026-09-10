mod checkpoints;
mod equipment;
mod falling;
mod group_respawn;
mod movement;
mod movement_reports;
mod outcomes;
mod plugin;
mod power_ups;
mod resources;
mod respawn;
mod spawning;
mod status;

#[cfg(test)]
#[path = "tests/checkpoints.rs"]
mod checkpoints_tests;
#[cfg(test)]
#[path = "tests/movement.rs"]
mod movement_tests;
#[cfg(test)]
#[path = "tests/resources.rs"]
mod resources_tests;
#[cfg(test)]
#[path = "tests/respawn.rs"]
pub(crate) mod respawn_tests;

pub use checkpoints::{CheckpointId, PlayerCheckpoint};
pub(crate) use checkpoints::{checkpoint_at_position, checkpoint_spawn_position, players_checkpoints_system};
pub use equipment::erase_equipment_system;
pub use falling::{players_fall_damage_system, players_fatal_outcomes_system};
pub(crate) use group_respawn::{enter_group_respawn, players_group_respawn_system};
pub(crate) use movement::apply_player_movement_system;
pub(crate) use movement_reports::queue_player_movement;
pub(crate) use outcomes::{PendingOutcomes, handle_move_outcome};
pub use plugin::players_plugin;
pub use power_ups::PowerUpState;

pub use resources::{Invincibility, PlayerInfo, PlayerMap, PlayerQuestState, PlayerStateQuery};
pub use respawn::players_respawn_system;
pub use status::players_status_timers_system;

pub(crate) use spawning::{PlayerSpawn, place_player_body, player_spawn_destination};
