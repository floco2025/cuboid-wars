use bevy::prelude::*;

use crate::{
    net::ClientToServer,
    resources::{ActorMap, FromClientsChannel, ItemMap, MapConfig, PlayerMap},
};
use common::{
    config::GameplayConfig,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorMarker, ItemMarker, MapLayout, PlayerMarker, *},
};

use super::{broadcast::broadcast_to_others, login::handle_login_message, messages::dispatch_message};

// Process incoming messages from clients.
// NOTE: Must run after `accept_connections_system` with `apply_deferred` in
// between, otherwise entities for the messages might not be spawned yet.
pub fn network_client_message_system(
    mut commands: Commands,
    mut from_clients: ResMut<FromClientsChannel>,
    mut players: ResMut<PlayerMap>,
    time: Res<Time>,
    map_layout: Res<MapLayout>,
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
                players.remove(&id);
                commands.entity(entity).despawn();

                debug!("{:?} disconnected (logged_in: {})", id, was_logged_in);

                if was_logged_in {
                    broadcast_to_others(&players, id, ServerMessage::Logoff(SLogoff { id, graceful: false }));
                }
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
