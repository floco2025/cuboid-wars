#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
#[allow(clippy::wildcard_imports)]
use rand::Rng as _;

use crate::{
    components::{LoggedIn, NetworkChannel},
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
        debug!("spawning entity for {:?}", event.id);
        let entity = commands
            .spawn((event.id, NetworkChannel(event.channel.clone())))
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
    player_query: Query<(Option<&LoggedIn>, &NetworkChannel)>,
    logged_in_query: Query<(&PlayerId, &NetworkChannel, &Position), With<LoggedIn>>,
) {
    for event in msg_message.read() {
        debug!("received message from {:?}: {:?}", event.id, event.message);
        
        if let Some(&entity) = player_index.0.get(&event.id) {
            if let Ok((logged_in, channel)) = player_query.get(entity) {
                if logged_in.is_some() {
                    handle_logged_in_message(&mut commands, entity, event.id, event.message.clone(), &logged_in_query);
                } else {
                    handle_not_logged_in_message(&mut commands, entity, event.id, event.message.clone(), channel, &logged_in_query);
                }
            }
        } else {
            error!("received message from unknown {:?}", event.id);
        }
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

fn handle_not_logged_in_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    channel: &NetworkChannel,
    logged_in_query: &Query<(&PlayerId, &NetworkChannel, &Position), With<LoggedIn>>,
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
            if let Err(e) = channel.0.send(ServerToClient::Send(init_msg)) {
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
                let _ = other_channel.0.send(ServerToClient::Send(login_msg.clone()));
            }
        }
        _ => {
            warn!("{:?} sent non-login message before authenticating", id);
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
            warn!("{:?} sent login after already authenticated", id);
            commands.entity(entity).despawn();
        }
        ClientMessage::Logoff(_) => {
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
