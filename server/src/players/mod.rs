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
#[path = "tests/fixtures.rs"]
pub(crate) mod fixtures;

pub use checkpoints::{CheckpointEntry, CheckpointId, PlayerCheckpoint};
pub(crate) use checkpoints::{
    checkpoint_at_position, checkpoint_numbered, checkpoint_progress, players_checkpoints_system,
};
pub use equipment::erase_equipment_system;
pub use falling::{players_fall_damage_system, players_fatal_outcomes_system};
pub(crate) use group_respawn::{enter_group_respawn, players_group_respawn_system};
pub(crate) use movement::apply_player_movement_system;
pub(crate) use movement_reports::queue_player_movement;
pub(crate) use outcomes::{PendingOutcomes, handle_move_outcome};
pub use plugin::players_plugin;
pub use power_ups::PowerUpState;

pub(crate) use resources::LoginStart;
pub use resources::{Invincibility, PlayerInfo, PlayerMap, PlayerQuestState, PlayerStateQuery};
pub use respawn::players_respawn_system;
pub use status::players_status_timers_system;

pub(crate) use spawning::{
    PlayerSpawn, occupied_player_positions, place_player_body, player_spawn_destination, start_destination,
};
