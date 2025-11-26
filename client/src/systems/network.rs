use bevy::prelude::*;
use common::protocol::{PlayerId, Position, Movement, ServerMessage};

use crate::{
    net::ServerToClient,
    resources::{MyPlayerId, ServerToClientChannel},
};

use super::spawning::{spawn_player, spawn_projectile_for_player};

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
    player_query: Query<(Entity, &PlayerId)>,
    player_pos_mov_query: Query<(&Position, &Movement), With<PlayerId>>,
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
                        &player_query,
                        &player_pos_mov_query,
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
    player_query: &Query<(Entity, &PlayerId)>,
    player_pos_mov_query: &Query<(&Position, &Movement), With<PlayerId>>,
    my_player_id: Option<PlayerId>,
    msg: &ServerMessage,
) {
    match msg {
        ServerMessage::Init(_) => {
            error!("received Init more than once");
        }
        ServerMessage::Login(msg) => {
            debug!("{:?} logged in", msg.id);
            // Login is always for another player (server doesn't send our own login back)
            spawn_player(
                commands,
                meshes,
                materials,
                msg.id.0,
                &msg.player.pos,
                &msg.player.mov,
                false, // Never local
            );
        }
        ServerMessage::Logoff(msg) => {
            debug!("{:?} logged off (graceful: {})", msg.id, msg.graceful);
            // Find and despawn the entity with this PlayerId
            for (entity, player_id) in player_query.iter() {
                if *player_id == msg.id {
                    commands.entity(entity).despawn();
                    break;
                }
            }
        }
        ServerMessage::Movement(msg) => {
            trace!("{:?} movement: {:?}", msg.id, msg);
            // Update player movement (both local and remote)
            let mut found = false;
            for (entity, player_id) in player_query.iter() {
                if *player_id == msg.id {
                    commands.entity(entity).insert(msg.mov);
                    found = true;
                    break;
                }
            }
            if !found {
                warn!("received movement for non-existent player {:?}", msg.id);
            }
        }
        ServerMessage::Shot(msg) => {
            trace!("{:?} shot: {:?}", msg.id, msg);
            // Update the shooter's movement first to sync exact facing direction
            let mut found = false;
            for (entity, player_id) in player_query.iter() {
                if *player_id == msg.id {
                    commands.entity(entity).insert(msg.mov);
                    found = true;
                    break;
                }
            }
            if !found {
                warn!("received shot from non-existent player {:?}", msg.id);
            }
            // Spawn projectile for this player
            spawn_projectile_for_player(commands, meshes, materials, player_query, player_pos_mov_query, msg.id);
        }
        ServerMessage::Update(msg) => {
            //trace!("update: {:?}", msg);

            // Get my player ID to identify local player
            let my_id: Option<u32> = my_player_id.map(|id| id.0);

            // Collect existing player IDs
            let existing_players: std::collections::HashSet<PlayerId> =
                player_query.iter().map(|(_, id)| *id).collect();

            // Collect player IDs in this Update message
            let update_players: std::collections::HashSet<PlayerId> = msg.players.iter().map(|(id, _)| *id).collect();

            // Spawn missing players (in Update but not in our world)
            for (id, player) in &msg.players {
                if !existing_players.contains(id) {
                    debug!("spawning player {:?} from Update", id);
                    let is_local = my_id.map_or(false, |my| my == (*id).0);
                    spawn_player(commands, meshes, materials, id.0, &player.pos, &player.mov, is_local);
                }
            }

            // Despawn players that no longer exist (in our world but not in Update)
            for (entity, player_id) in player_query.iter() {
                if !update_players.contains(player_id) {
                    warn!("despawning player {:?} from Update", player_id);
                    commands.entity(entity).despawn();
                }
            }

            // Update all players with new state
            for (id, player) in &msg.players {
                for (entity, player_id) in player_query.iter() {
                    if *player_id == *id {
                        commands.entity(entity).insert((player.pos, player.mov));
                        break;
                    }
                }
            }
        }
    }
}
