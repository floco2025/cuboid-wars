#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
#[allow(clippy::wildcard_imports)]
use rand::Rng as _;

use crate::{
    components::{Connected, LoggedIn, NetworkChannel},
    messages::{ClientConnected, ClientDisconnected, ClientMessageReceived},
    net::{ClientToServer, ServerToClient},
    resources::{ClientToServerChannel, PlayerIndex},
};
use common::protocol::*;

// ============================================================================
// Network Receiver System
// ============================================================================

/// System to receive messages from the channel and convert them to Bevy events
/// This runs first and feeds events to other systems
pub fn network_receiver_system(
    mut from_clients: ResMut<ClientToServerChannel>,
    mut msg_connected: MessageWriter<ClientConnected>,
    mut msg_disconnected: MessageWriter<ClientDisconnected>,
    mut msg_message: MessageWriter<ClientMessageReceived>,
) {
    while let Ok((id, msg)) = from_clients.try_recv() {
        match msg {
            ClientToServer::Connected(channel) => {
                msg_connected.write(ClientConnected { id, channel });
            }
            ClientToServer::Disconnected => {
                msg_disconnected.write(ClientDisconnected { id });
            }
            ClientToServer::Message(message) => {
                msg_message.write(ClientMessageReceived { id, message });
            }
        }
    }
}

// ============================================================================
// Connection Management Systems
// ============================================================================

/// System to handle connection and disconnection events
pub fn handle_connections_system(
    mut commands: Commands,
    mut player_index: ResMut<PlayerIndex>,
    mut msg_connected: MessageReader<ClientConnected>,
    mut msg_disconnected: MessageReader<ClientDisconnected>,
    logged_in_query: Query<(&PlayerId, &NetworkChannel, &Position), With<LoggedIn>>,
) {
    // Handle connections
    for event in msg_connected.read() {
        debug!("spawning entity for player {:?}", event.id);
        let entity = commands
            .spawn((event.id, NetworkChannel(event.channel.clone()), Connected))
            .id();
        player_index.0.insert(event.id, entity);
    }

    // Handle disconnections
    for event in msg_disconnected.read() {
        if let Some(entity) = player_index.0.remove(&event.id) {
            let was_logged_in = logged_in_query.get(entity).is_ok();

            debug!("client {:?} disconnected (logged_in: {})", event.id, was_logged_in);
            commands.entity(entity).despawn();

            // Broadcast logoff to all other logged-in players if they were logged in
            if was_logged_in {
                let logoff_msg = ServerMessage::Logoff(SLogoff {
                    id: event.id,
                    graceful: false,
                });
                for (other_id, other_channel, _) in logged_in_query.iter() {
                    if *other_id != event.id {
                        let _ = other_channel.0.send(ServerToClient::Send(logoff_msg.clone()));
                    }
                }
            }
        }
    }
}

/// System to handle all messages from clients (runs after handle_connections_system)
pub fn process_client_messages_system(
    mut commands: Commands,
    mut msg_message: MessageReader<ClientMessageReceived>,
    player_index: Res<PlayerIndex>,
    connected_query: Query<&NetworkChannel, (With<Connected>, Without<LoggedIn>)>,
    logged_in_query: Query<(&PlayerId, &NetworkChannel, &Position), With<LoggedIn>>,
) {
    for event in msg_message.read() {
        debug!("received message from player {:?}: {:?}", event.id, event.message);
        
        // Fast O(1) lookup using index
        if let Some(&entity) = player_index.0.get(&event.id) {
            debug!("found entity {:?} for player {:?}", entity, event.id);
            
            // Check if connected or logged in
            let is_connected = connected_query.get(entity).is_ok();
            let is_logged_in = logged_in_query.get(entity).is_ok();

            if is_connected {
                debug!("player {:?} is in connected state", event.id);
                let channel = connected_query.get(entity).unwrap();
                handle_connected_message(
                    &mut commands,
                    entity,
                    event.id,
                    event.message.clone(),
                    channel,
                    &logged_in_query,
                );
            } else if is_logged_in {
                debug!("player {:?} is in logged-in state", event.id);
                handle_logged_in_message(
                    &mut commands,
                    entity,
                    event.id,
                    event.message.clone(),
                    &logged_in_query,
                );
            } else {
                // Entity exists but components haven't been applied yet
                warn!(
                    "player {:?} entity {:?} components not yet applied, message will be lost: {:?}",
                    event.id, entity, event.message
                );  
            }
        } else {
            warn!("received message from unknown player {:?}", event.id);
        }
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

fn handle_connected_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    channel: &NetworkChannel,
    logged_in_query: &Query<(&PlayerId, &NetworkChannel, &Position), With<LoggedIn>>,
) {
    debug!("handling connected message for {:?}: {:?}", id, msg);
    match msg {
        ClientMessage::Login(_) => {
            debug!("player {:?} logging in", id);

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
            debug!("sending init message to {:?}: {:?}", id, init_msg);
            if let Err(e) = channel.0.send(ServerToClient::Send(init_msg)) {
                warn!("failed to send init to player {:?}: {}", id, e);
                return;
            }

            // Update entity: remove Connected, add LoggedIn + Position
            commands.entity(entity).remove::<Connected>().insert((LoggedIn, pos));

            // Broadcast Login to all other logged-in players
            let login_msg = ServerMessage::Login(SLogin {
                id,
                player: Player { pos },
            });
            for (_, other_channel, _) in logged_in_query.iter() {
                let _ = other_channel.0.send(ServerToClient::Send(login_msg.clone()));
            }
        }
        _ => {
            warn!("player {:?} sent non-login message before authenticating", id);
            commands.entity(entity).despawn();
        }
    }
}

fn handle_logged_in_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    logged_in_query: &Query<(&PlayerId, &NetworkChannel, &Position), With<LoggedIn>>,
) {
    match msg {
        ClientMessage::Login(_) => {
            warn!("player {:?} sent login after already authenticated", id);
            commands.entity(entity).despawn();
        }
        ClientMessage::Logoff(_) => {
            debug!("player {:?} requested graceful logoff", id);
            commands.entity(entity).despawn();

            // Broadcast graceful logoff to all other players
            let logoff_msg = ServerMessage::Logoff(SLogoff { id, graceful: true });
            for (other_id, other_channel, _) in logged_in_query.iter() {
                if *other_id != id {
                    let _ = other_channel.0.send(ServerToClient::Send(logoff_msg.clone()));
                }
            }
        }
    }
}
