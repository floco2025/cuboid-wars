use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    config::ServerGameplayConfig,
    net::ClientToServer,
    resources::{ActorMap, FromClientsChannel, ItemMap, MapConfig, PlayerInfo, PlayerMap},
};
use common::{
    config::GameplayConfig,
    map_geometry::MapGeometry,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorMarker, ItemMarker, MapLayout, PlayerMarker, *},
};

use super::{login::handle_login_message, messages::dispatch_message};

// Bundled login dependencies — keeps `network_process_client_messages_system`
// under Bevy's 16-parameter system tuple limit. All resources here are pure
// world setup / config that login flow needs in a single shot.
#[derive(SystemParam)]
pub struct LoginWorld<'w> {
    pub map_layout: Res<'w, MapLayout>,
    pub map_geometry: Res<'w, MapGeometry>,
    pub collision_world: Res<'w, CollisionWorld>,
    pub gameplay_config: Res<'w, GameplayConfig>,
    pub map_config: Res<'w, MapConfig>,
    pub server_gameplay_config: Res<'w, ServerGameplayConfig>,
}

// Process incoming messages from clients.
// NOTE: Must run after `network_accept_connections_system` with `apply_deferred` in
// between, otherwise entities for the messages might not be spawned yet.
pub fn network_process_client_messages_system(
    mut commands: Commands,
    mut from_clients: ResMut<FromClientsChannel>,
    mut players: ResMut<PlayerMap>,
    time: Res<Time>,
    world: LoginWorld,
    items: Res<ItemMap>,
    actors: Res<ActorMap>,
    player_data: Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    player_motions: Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
    actor_data: Query<(&Position, &ActorMoveIntent, &FaceDirection, &Health), With<ActorMarker>>,
    actor_motions: Query<&CharacterVerticalVelocity, With<ActorMarker>>,
    item_data: Query<&Position, With<ItemMarker>>,
) {
    while let Ok((id, event)) = from_clients.try_recv() {
        if let ClientToServer::Registration { to_client } = event {
            debug!("{:?} registered", id);
            let entity = commands.spawn((PlayerMarker, id)).id();
            players.insert(id, PlayerInfo::new(entity, to_client));
            continue;
        }

        let Some(player_info) = players.get(&id) else {
            error!("received event for unknown {:?}", id);
            continue;
        };

        match event {
            ClientToServer::Registration { .. } => unreachable!("handled above"),
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
                // Other clients notice the absence on the next `SSnapshot`.
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
                        &world.collision_world,
                        &world.gameplay_config,
                    );
                } else {
                    handle_login_message(
                        &mut commands,
                        player_info.entity,
                        id,
                        message,
                        &mut players,
                        &world,
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
