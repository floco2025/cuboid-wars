#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
#[allow(clippy::wildcard_imports)]
use rand::Rng as _;

use crate::{
    components::{LoggedIn, ServerToClientChannel},
    net::{ClientToServer, ServerToClient},
    resources::{FromAcceptChannel, FromClientsChannel, PlayerIndex},
};
use common::protocol::*;

// ============================================================================
// Network Systems
// ============================================================================

/// System to process new client connections and spawn entities.
pub fn process_new_connections_system(
    mut commands: Commands,
    mut from_accept: ResMut<FromAcceptChannel>,
    mut player_index: ResMut<PlayerIndex>,
) {
    while let Ok((id, to_client)) = from_accept.try_recv() {
        debug!("spawning entity for {:?}", id);
        let entity = commands.spawn((id, ServerToClientChannel::new(to_client))).id();
        player_index.0.insert(id, entity);
    }
}

/// System to process client events (messages and disconnections). Must run after
/// process_new_connections_system with apply_deferred in between.
pub fn process_client_message_system(
    mut commands: Commands,
    mut from_clients: ResMut<FromClientsChannel>,
    mut player_index: ResMut<PlayerIndex>,
    player_query: Query<(Option<&LoggedIn>, &ServerToClientChannel)>,
    logged_in_query: Query<(&PlayerId, &ServerToClientChannel, &Position), With<LoggedIn>>,
) {
    while let Ok((id, event)) = from_clients.try_recv() {
        match event {
            ClientToServer::Disconnected => {
                if let Some(entity) = player_index.0.remove(&id) {
                    let was_logged_in = logged_in_query.get(entity).is_ok();

                    debug!("client {:?} disconnected (logged_in: {})", id, was_logged_in);
                    commands.entity(entity).despawn();

                    // Broadcast logoff to all other logged-in players if they were logged in
                    if was_logged_in {
                        let logoff_msg = ServerMessage::Logoff(SLogoff { id, graceful: false });
                        for (other_id, other_channel, _) in logged_in_query.iter() {
                            if *other_id != id {
                                let _ = other_channel.send(ServerToClient::Send(logoff_msg.clone()));
                            }
                        }
                    }
                }
            }
            ClientToServer::Message(message) => {
                debug!("received message from {:?}: {:?}", id, message);

                if let Some(&entity) = player_index.0.get(&id) {
                    if let Ok((logged_in, channel)) = player_query.get(entity) {
                        if logged_in.is_some() {
                            process_message_logged_in(&mut commands, entity, id, message, &logged_in_query);
                        } else {
                            process_message_not_logged_in(&mut commands, entity, id, message, channel, &logged_in_query);
                        }
                    }
                } else {
                    error!("received message from unknown {:?}", id);
                }
            }
        }
    }
}

// ============================================================================
// Dispatch Messages
// ============================================================================

fn process_message_not_logged_in(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    channel: &ServerToClientChannel,
    logged_in_query: &Query<(&PlayerId, &ServerToClientChannel, &Position), With<LoggedIn>>,
) {
    match msg {
        ClientMessage::Login(_) => {
            // Get all currently logged-in players
            let other_players: Vec<(PlayerId, Player)> = logged_in_query
                .iter()
                .map(|(id, _, pos)| (*id, Player { pos: *pos }))
                .collect();

            // Generate random position for new player
            let mut rng = rand::rng();
            let pos = Position {
                x: rng.random_range(-1000..=1000),
                y: rng.random_range(-1000..=1000),
            };

            // Send Init to the connecting player
            let init_msg = ServerMessage::Init(SInit {
                id,
                player: Player { pos },
                other_players,
            });
            if let Err(e) = channel.send(ServerToClient::Send(init_msg)) {
                warn!("failed to send init to {:?}: {}", id, e);
                return;
            }

            // Update entity: add LoggedIn + Position
            commands.entity(entity).insert((LoggedIn, pos));

            // Broadcast Login to all other logged-in players
            let login_msg = ServerMessage::Login(SLogin {
                id,
                player: Player { pos },
            });
            for (_, other_channel, _) in logged_in_query.iter() {
                let _ = other_channel.send(ServerToClient::Send(login_msg.clone()));
            }
        }
        _ => {
            warn!("{:?} sent non-login message before authenticating", id);
            commands.entity(entity).despawn();
        }
    }
}

fn process_message_logged_in(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    logged_in_query: &Query<(&PlayerId, &ServerToClientChannel, &Position), With<LoggedIn>>,
) {
    match msg {
        ClientMessage::Login(_) => {
            warn!("{:?} sent login after already authenticated", id);
            commands.entity(entity).despawn();
        }
        ClientMessage::Logoff(_) => {
            commands.entity(entity).despawn();

            // Broadcast graceful logoff to all other players
            let logoff_msg = ServerMessage::Logoff(SLogoff { id, graceful: true });
            for (other_id, other_channel, _) in logged_in_query.iter() {
                if *other_id != id {
                    let _ = other_channel.send(ServerToClient::Send(logoff_msg.clone()));
                }
            }
        }
    }
}
