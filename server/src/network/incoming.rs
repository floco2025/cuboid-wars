use bevy::prelude::*;

use crate::{
    net::ClientToServer,
    resources::{ActorMap, FromClientsChannel, ItemMap, MapConfig, PlayerMap},
};
use common::{
    config::GameplayConfig,
    map_geometry::MapGeometry,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorMarker, ItemMarker, MapLayout, PlayerMarker, *},
};

use super::{login::handle_login_message, messages::dispatch_message};

// Process incoming messages from clients.
// NOTE: Must run after `network_accept_connections_system` with `apply_deferred` in
// between, otherwise entities for the messages might not be spawned yet.
pub fn network_process_client_messages_system(
    mut commands: Commands,
    mut from_clients: ResMut<FromClientsChannel>,
    mut players: ResMut<PlayerMap>,
    time: Res<Time>,
    map_layout: Res<MapLayout>,
    map_geometry: Res<MapGeometry>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    map_config: Res<MapConfig>,
    items: Res<ItemMap>,
    actors: Res<ActorMap>,
    player_data: Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    player_motions: Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
    actor_data: Query<(&Position, &ActorMoveIntent, &FaceDirection, &Health), With<ActorMarker>>,
    actor_motions: Query<&CharacterVerticalVelocity, With<ActorMarker>>,
    item_data: Query<&Position, With<ItemMarker>>,
) {
    while let Ok((id, event)) = from_clients.try_recv() {
        let Some(player_info) = players.get(&id) else {
            error!("received event for unknown {:?}", id);
            continue;
        };

        match event {
            ClientToServer::Disconnected => {
                let was_logged_in = player_info.logged_in;
                let entity = player_info.entity;
                let was_dead = player_info.is_dead();
                players.remove(&id);
                // If the player was mid-death their entity is already
                // despawned; skip the redundant despawn so we don't panic on
                // a stale handle.
                if !was_dead {
                    commands.entity(entity).despawn();
                }

                debug!("{:?} disconnected (logged_in: {})", id, was_logged_in);
                // Other clients notice the absence on the next `SUpdate`.
            }
            ClientToServer::Message(message) => {
                let is_logged_in = player_info.logged_in;
                if is_logged_in {
                    dispatch_message(
                        &mut commands,
                        player_info.entity,
                        id,
                        message,
                        &mut players,
                        &time,
                        &player_data,
                        &player_motions,
                        &collision_world,
                        &gameplay_config,
                    );
                } else {
                    handle_login_message(
                        &mut commands,
                        player_info.entity,
                        id,
                        message,
                        &mut players,
                        &map_layout,
                        &map_geometry,
                        &collision_world,
                        &gameplay_config,
                        &map_config,
                        &items,
                        &actors,
                        &player_data,
                        &player_motions,
                        &actor_data,
                        &actor_motions,
                        &item_data,
                    );
                }
            }
        }
    }
}
