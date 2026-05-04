use bevy::prelude::*;

use crate::{
    actors::{actor_behavior_system, actor_death_system, actor_initial_spawn_system, actor_respawn_system},
    characters::{characters_health_regeneration_system, characters_movement_system},
    items::{
        item_collection_system, item_despawn_system, item_initial_spawn_system, item_respawn_system, item_spawn_system,
    },
    network::{network_accept_connections_system, network_broadcast_state_system, network_client_message_system},
    players::{players_fall_recovery_system, players_timer_system},
    projectiles::projectiles_movement_system,
};

pub struct ServerGamePlugin;

impl Plugin for ServerGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, actor_initial_spawn_system).add_systems(
            Update,
            (
                // Network systems must run in order:
                // 1. Accept new connections (spawns entities)
                // 2. ApplyDeferred (makes entities queryable)
                // 3. Process client messages (needs to query those entities)
                // 4. ApplyDeferred (makes message-side component changes queryable)
                // 5. Broadcast state to all clients
                (
                    network_accept_connections_system,
                    ApplyDeferred,
                    network_client_message_system,
                    ApplyDeferred,
                    network_broadcast_state_system,
                )
                    .chain(),
                // Game logic systems can run in parallel.
                characters_movement_system.after(actor_behavior_system),
                players_timer_system,
                // Fall recovery must run after movement updates positions.
                players_fall_recovery_system.after(characters_movement_system),
                actor_respawn_system,
                actor_behavior_system,
                actor_death_system
                    .after(characters_movement_system)
                    .after(projectiles_movement_system)
                    .before(characters_health_regeneration_system),
                projectiles_movement_system,
                characters_health_regeneration_system
                    .after(characters_movement_system)
                    .after(projectiles_movement_system),
                item_initial_spawn_system,
                item_spawn_system,
                item_despawn_system,
                item_collection_system,
                item_respawn_system,
            ),
        );
    }
}
