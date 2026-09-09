mod checkpoints;
mod equipment;
mod falling;
mod group_respawn;
mod plugin;
mod power_ups;
mod resources;
mod respawn;
mod status;

#[cfg(test)]
mod checkpoints_tests;
#[cfg(test)]
mod resources_tests;
#[cfg(test)]
mod respawn_tests;

pub use checkpoints::{CheckpointId, PlayerCheckpoint};
pub(crate) use checkpoints::{checkpoint_spawn_position, players_checkpoints_system};
pub use equipment::{EraserContacts, erase_equipment_system};
pub use falling::{PlayerFallState, players_fall_damage_system, players_fall_death_system};
pub(crate) use group_respawn::{enter_group_respawn, players_group_respawn_system};
pub use plugin::players_plugin;
pub use power_ups::PowerUpState;

pub use resources::{
    Invincibility, PlayerConnection, PlayerInfo, PlayerLife, PlayerMap, PlayerQuestState, PlayerSession,
    PlayerStateQuery,
};
pub use respawn::players_respawn_system;
pub use status::players_status_timers_system;
