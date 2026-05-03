mod actors;
mod characters;
mod items;
mod network;
mod players;
mod projectiles;

pub use actors::{actor_behavior_system, actor_death_system, actor_spawn_quota_system};
pub use characters::{characters_health_regeneration_system, characters_movement_system};
// generate_player_spawn_position is re-exported here for the network
// login.rs which goes through `systems::*`. The actor variant is reached
// via `systems::characters::...` directly so doesn't need to be re-exported.
pub(crate) use characters::generate_player_spawn_position;
pub use items::{
    item_collection_system, item_despawn_system, item_initial_spawn_system, item_respawn_system, item_spawn_system,
};
pub use network::{
    broadcast_to_all, network_accept_connections_system, network_broadcast_state_system, network_client_message_system,
};
pub use players::{players_fall_recovery_system, players_timer_system};
pub use projectiles::projectiles_movement_system;
