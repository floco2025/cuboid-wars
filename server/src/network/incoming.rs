use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    actors::{ActorMap, PendingActorSpawns},
    config::ServerGameplayConfig,
    items::ItemMap,
    map::MapConfig,
    map::OpenBarrierKinds,
    missiles::MissileMap,
    network::{ClientToServer, FromClientsChannel, announce},
    players::{PlayerInfo, PlayerMap},
};

use super::admin::AdminContext;
use common::{
    config::GameplayConfig,
    map::MapGeometry,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorMarker, ItemMarker, MapLayout, PlayerMarker, *},
};

use super::{login::handle_login_message, messages::dispatch_message};

// The full player/actor per-entity state queries, spelled once. Signature
// noise elsewhere threads these constantly; keep new consumers on the alias.
pub type PlayerStateQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        &'static PlayerMoveIntent,
        &'static FaceYaw,
        &'static Health,
    ),
    With<PlayerMarker>,
>;

pub type ActorStateQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        &'static ActorMoveIntent,
        &'static FaceYaw,
        &'static Health,
    ),
    With<ActorMarker>,
>;

// Bundled world/config resources shared by login and message dispatch —
// keeps `network_process_client_messages_system` under Bevy's 16-parameter
// system tuple limit, and turns "a handler needs one more resource" into a
// one-field change instead of a three-layer signature edit.
#[derive(SystemParam)]
pub struct SharedWorld<'w> {
    pub map_layout: Res<'w, MapLayout>,
    pub map_settings: Res<'w, MapSettings>,
    pub map_geometry: Res<'w, MapGeometry>,
    pub collision_world: Res<'w, CollisionWorld>,
    pub gameplay_config: Res<'w, GameplayConfig>,
    pub map_config: Res<'w, MapConfig>,
    pub server_gameplay_config: Res<'w, ServerGameplayConfig>,
}

// The character state/motion queries every message handler reads.
#[derive(SystemParam)]
pub struct CharacterQueries<'w, 's> {
    pub player_data: PlayerStateQuery<'w, 's>,
    pub player_motions: Query<'w, 's, &'static CharacterVerticalVelocity, With<PlayerMarker>>,
    pub actor_data: ActorStateQuery<'w, 's>,
    pub actor_motions: Query<'w, 's, &'static CharacterVerticalVelocity, With<ActorMarker>>,
}

// Process incoming messages from clients. Registrations and messages share
// one channel (see `transport.rs`), so a player's `Registration` — and its
// entity spawn here — is always observed before any of their messages.
pub fn network_process_client_messages_system(
    mut commands: Commands,
    mut from_clients: ResMut<FromClientsChannel>,
    mut players: ResMut<PlayerMap>,
    time: Res<Time>,
    world: SharedWorld,
    queries: CharacterQueries,
    items: Res<ItemMap>,
    actors: Res<ActorMap>,
    item_data: Query<&Position, With<ItemMarker>>,
    open_barrier_kinds: Res<OpenBarrierKinds>,
    mut missiles: ResMut<MissileMap>,
    mut pending_actor_spawns: ResMut<PendingActorSpawns>,
    mut admin: AdminContext,
) {
    while let Ok((id, event)) = from_clients.try_recv() {
        if let ClientToServer::Registration { to_client } = event {
            debug!("player#{} registered", id.0);
            let entity = commands.spawn((PlayerMarker, id)).id();
            players.insert(id, PlayerInfo::new(entity, to_client));
            continue;
        }

        let Some(player_info) = players.get(&id) else {
            error!("received event for unknown player#{}", id.0);
            continue;
        };

        match event {
            ClientToServer::Registration { .. } => unreachable!("handled above"),
            ClientToServer::Disconnected => {
                let who = players.describe(&id);
                let name = players.display_name(&id);
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

                debug!("{} disconnected (logged_in: {})", who, was_logged_in);
                // Presence is snapshot-only — other clients notice the
                // absence on the next `SSnapshot`; the feed line is cosmetic.
                if was_logged_in {
                    announce(
                        &players,
                        &world.server_gameplay_config.feed,
                        FeedEvent::PlayerLeft { name },
                    );
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
                        &mut missiles,
                        &time,
                        &world,
                        &queries,
                        &actors,
                        &open_barrier_kinds,
                        &mut pending_actor_spawns,
                        &mut admin,
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
                        &pending_actor_spawns,
                        &queries,
                        &item_data,
                        admin.weather.intensity(),
                        admin.light.blend(),
                    );
                }
            }
        }
    }
}
