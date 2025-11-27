use bevy::prelude::*;

use super::effects::{CameraShake, CuboidShake};
use crate::spawning::{spawn_player, spawn_projectile_for_player};
use crate::{
    net::ServerToClient,
    resources::{MyPlayerId, PlayerInfo, PlayerMap, ServerToClientChannel},
};
use common::protocol::{Movement, PlayerId, Position, ServerMessage};

// ============================================================================
// Network Message Processing
// ============================================================================

// System to process messages from the server
pub fn process_server_events_system(
    mut commands: Commands,
    mut from_server: ResMut<ServerToClientChannel>,
    mut exit: MessageWriter<AppExit>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut player_map: ResMut<PlayerMap>,
    player_pos_mov_query: Query<(&Position, &Movement), With<PlayerId>>,
    camera_query: Query<Entity, With<Camera3d>>,
    mut my_player_id: Local<Option<PlayerId>>,
) {
    // Process all messages from the server
    while let Ok(msg) = from_server.try_recv() {
        match msg {
            ServerToClient::Disconnected => {
                error!("disconnected from server");
                exit.write(AppExit::Success);
            }
            ServerToClient::Message(message) => {
                if my_player_id.is_some() {
                    process_message_logged_in(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &mut player_map,
                        &player_pos_mov_query,
                        &camera_query,
                        *my_player_id,
                        &message,
                    );
                } else {
                    process_message_not_logged_in(&mut commands, &message, &mut my_player_id);
                }
            }
        }
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

fn process_message_not_logged_in(
    commands: &mut Commands,
    msg: &ServerMessage,
    my_player_id: &mut Local<Option<PlayerId>>,
) {
    match msg {
        ServerMessage::Init(init_msg) => {
            debug!("received Init: my_id={:?}", init_msg.id);

            // Store in Local (immediate) and insert resource (deferred)
            **my_player_id = Some(init_msg.id);
            commands.insert_resource(MyPlayerId(init_msg.id));

            // Note: We don't spawn anything here. The first SUpdate will contain
            // all players including ourselves and will trigger spawning via the
            // Update message handler.
        }
        _ => {
            warn!("received non-Init message before Init (out-of-order delivery)");
        }
    }
}

fn process_message_logged_in(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    players: &mut ResMut<PlayerMap>,
    player_pos_mov_query: &Query<(&Position, &Movement), With<PlayerId>>,
    camera_query: &Query<Entity, With<Camera3d>>,
    my_player_id: Option<PlayerId>,
    msg: &ServerMessage,
) {
    match msg {
        ServerMessage::Init(_) => {
            error!("received Init more than once");
        }
        ServerMessage::Login(msg) => {
            debug!("{:?} logged in", msg.id);
            // Spawn the new player if not already in player_map
            if !players.0.contains_key(&msg.id) {
                let entity = spawn_player(
                    commands,
                    meshes,
                    materials,
                    msg.id.0,
                    &msg.player.pos,
                    &msg.player.mov,
                    false, // Never local (server doesn't send our own login back)
                );
                players.0.insert(msg.id, PlayerInfo { entity, hits: 0 });
            }
        }
        ServerMessage::Logoff(msg) => {
            debug!("{:?} logged off (graceful: {})", msg.id, msg.graceful);
            // Remove from player map and despawn entity
            if let Some(player) = players.0.remove(&msg.id) {
                commands.entity(player.entity).despawn();
            }
        }
        ServerMessage::Movement(msg) => {
            trace!("{:?} movement: {:?}", msg.id, msg);
            // Update player movement using player_map
            if let Some(player) = players.0.get(&msg.id) {
                commands.entity(player.entity).insert(msg.mov);
            } else {
                warn!("received movement for non-existent player {:?}", msg.id);
            }
        }
        ServerMessage::Shot(msg) => {
            trace!("{:?} shot: {:?}", msg.id, msg);
            // Update the shooter's movement first to sync exact facing direction
            if let Some(player) = players.0.get(&msg.id) {
                commands.entity(player.entity).insert(msg.mov);
                // Spawn projectile for this player
                spawn_projectile_for_player(commands, meshes, materials, player_pos_mov_query, player.entity);
            } else {
                warn!("received shot from non-existent player {:?}", msg.id);
            }
        }
        ServerMessage::Update(msg) => {
            //trace!("update: {:?}", msg);

            // Get my player ID to identify local player
            let my_id: Option<u32> = my_player_id.map(|id| id.0);

            // Collect player IDs in this Update message
            let update_players: std::collections::HashSet<PlayerId> = msg.players.iter().map(|(id, _)| *id).collect();

            // Spawn missing players (in Update but not in player_map)
            for (id, player) in &msg.players {
                if !players.0.contains_key(id) {
                    let is_local = my_id.map_or(false, |my| my == (*id).0);
                    debug!("spawning player {:?} from Update (is_local: {})", id, is_local);
                    let entity = spawn_player(commands, meshes, materials, id.0, &player.pos, &player.mov, is_local);
                    players.0.insert(
                        *id,
                        PlayerInfo {
                            entity,
                            hits: player.hits,
                        },
                    );
                }
            }

            // Despawn players that no longer exist (in player_map but not in Update)
            let to_remove: Vec<PlayerId> = players
                .0
                .keys()
                .filter(|id| !update_players.contains(id))
                .copied()
                .collect();

            for id in to_remove {
                if let Some(player) = players.0.remove(&id) {
                    warn!("despawning player {:?} from Update", id);
                    commands.entity(player.entity).despawn();
                }
            }

            // Update all players with new state
            for (id, server_player) in &msg.players {
                if let Some(client_player) = players.0.get_mut(id) {
                    commands
                        .entity(client_player.entity)
                        .insert((server_player.pos, server_player.mov));
                    // Update hit count from server
                    client_player.hits = server_player.hits;
                }
            }
        }
        ServerMessage::Hit(msg) => {
            debug!("player {:?} was hit", msg.id);
            // Check if it's the local player
            if Some(msg.id) == my_player_id {
                // Shake camera for local player
                if let Ok(camera_entity) = camera_query.single() {
                    commands.entity(camera_entity).insert(CameraShake {
                        timer: Timer::from_seconds(0.3, TimerMode::Once),
                        intensity: 3.0,
                        direction_x: msg.hit_dir_x,
                        direction_z: msg.hit_dir_z,
                        offset_x: 0.0,
                        offset_y: 0.0,
                        offset_z: 0.0,
                    });
                }
            } else {
                // Shake cuboid for other players
                if let Some(player) = players.0.get(&msg.id) {
                    commands.entity(player.entity).insert(CuboidShake {
                        timer: Timer::from_seconds(0.3, TimerMode::Once),
                        intensity: 0.3,
                        direction_x: msg.hit_dir_x,
                        direction_z: msg.hit_dir_z,
                        offset_x: 0.0,
                        offset_z: 0.0,
                    });
                }
            }
        }
    }
}
