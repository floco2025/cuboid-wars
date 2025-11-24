use bevy::prelude::*;
use crate::client::ClientState;
use crate::world::{LocalPlayer, spawn_player};
use common::protocol::PlayerId;
use std::collections::HashSet;

// ============================================================================
// Player Synchronization System
// ============================================================================

/// Synchronizes player entities with ClientState
pub fn sync_players(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    client_state: Res<ClientState>,
    player_entities: Query<(Entity, &PlayerId)>,
) {
    // Get IDs of all players in the game state
    let state_player_ids: HashSet<PlayerId> = client_state.players().keys().copied().collect();
    
    // Get IDs of all existing player entities
    let entity_player_ids: HashSet<PlayerId> = player_entities
        .iter()
        .map(|(_, p)| *p)
        .collect();

    // Spawn new players that don't have entities yet
    for &player_id in &state_player_ids {
        if !entity_player_ids.contains(&player_id) {
            if let Some(player) = client_state.players().get(&player_id) {
                let is_local = client_state.my_id() == Some(player_id);
                spawn_player(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    player_id.0,
                    &player.pos,
                    is_local,
                );
            }
        }
    }

    // Despawn players that are no longer in the game state
    for (entity, player_entity) in &player_entities {
        if !state_player_ids.contains(player_entity) {
            commands.entity(entity).despawn();
        }
    }
}

/// Update player positions based on ClientState
pub fn update_player_positions(
    client_state: Res<ClientState>,
    mut player_query: Query<(&PlayerId, &mut Transform), Without<LocalPlayer>>,
) {
    for (player_id, mut transform) in &mut player_query {
        if let Some(player) = client_state.players().get(player_id) {
            transform.translation.x = player.pos.x as f32;
            transform.translation.z = player.pos.y as f32;
        }
    }
}
