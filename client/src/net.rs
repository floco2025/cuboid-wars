use crate::world;
#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use common::net::MessageStream;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use quinn::{Connection, ConnectionError};
use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender,
    error::{SendError, TryRecvError},
};

// ============================================================================
// Resources
// ============================================================================

/// My player ID assigned by the server
#[derive(Resource)]
pub struct MyPlayerId(pub PlayerId);

/// A resource wrapper for the bevy to server channel
#[derive(Resource)]
pub struct BevyToServerChannel(UnboundedSender<BevyToServer>);

impl BevyToServerChannel {
    pub fn new(sender: UnboundedSender<BevyToServer>) -> Self {
        Self(sender)
    }

    pub fn send(&self, msg: BevyToServer) -> Result<(), SendError<BevyToServer>> {
        self.0.send(msg)
    }
}

/// A resource wrapper for the server to bevy channel
#[derive(Resource)]
pub struct ServerToBevyChannel(UnboundedReceiver<ServerToBevy>);

impl ServerToBevyChannel {
    pub fn new(receiver: UnboundedReceiver<ServerToBevy>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<ServerToBevy, TryRecvError> {
        self.0.try_recv()
    }
}

// ============================================================================
// Network I/O Task
// ============================================================================

/// Message from network I/O task to Bevy main thread
#[derive(Debug, Clone)]
pub enum ServerToBevy {
    Message(ServerMessage),
    Disconnected,
}

/// Message from Bevy main thread to network I/O task
#[derive(Debug, Clone)]
pub enum BevyToServer {
    Send(ClientMessage),
    Close,
}

pub async fn network_io_task(
    connection: Connection,
    to_bevy: UnboundedSender<ServerToBevy>,
    mut from_bevy: UnboundedReceiver<BevyToServer>,
) {
    let stream = MessageStream::new(&connection);

    loop {
        tokio::select! {
            // Receive from server
            result = stream.recv::<ServerMessage>() => {
                match result {
                    Ok(msg) => {
                        if to_bevy.send(ServerToBevy::Message(msg)).is_err() {
                            // Bevy side closed, exit
                            break;
                        }
                    }
                    Err(e) => {
                        if let Some(conn_err) = e.downcast_ref::<ConnectionError>() {
                            match conn_err {
                                ConnectionError::ApplicationClosed { .. } => {
                                    eprintln!("Server closed the connection");
                                }
                                ConnectionError::TimedOut => {
                                    eprintln!("Server connection timed out");
                                }
                                ConnectionError::LocallyClosed => {
                                    eprintln!("Connection to server closed locally");
                                }
                                _ => {
                                    eprintln!("Connection error: {e}");
                                }
                            }
                        } else {
                            eprintln!("Error receiving message: {e}");
                        }
                        break;
                    }
                }
            }

            // Send to server
            cmd = from_bevy.recv() => {
                match cmd {
                    Some(BevyToServer::Send(msg)) => {
                        if let Err(e) = stream.send(&msg).await {
                            eprintln!("Error sending to server: {e}");
                            break;
                        }
                    }
                    Some(BevyToServer::Close) => {
                        connection.close(0u32.into(), b"client closing");
                        break;
                    }
                    None => {
                        // Bevy side closed, exit
                        break;
                    }
                }
            }
        }
    }

    // Notify Bevy that we're disconnected
    let _ = to_bevy.send(ServerToBevy::Disconnected);
}

// ============================================================================
// Network Polling System
// ============================================================================

/// This system processes messages from the server and spawns/despawns entities directly
pub fn server_to_bevy_system(
    mut commands: Commands,
    mut from_server: ResMut<ServerToBevyChannel>,
    mut exit: EventWriter<AppExit>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player_query: Query<(Entity, &PlayerId)>,
) {
    // Process all available messages
    while let Ok(msg) = from_server.try_recv() {
        match msg {
            ServerToBevy::Message(server_msg) => {
                match server_msg {
                    ServerMessage::Init(init_msg) => {
                        info!(
                            "Received Init: my_id={:?}, {} existing players",
                            init_msg.id,
                            init_msg.other_players.len()
                        );

                        // Insert MyPlayerId resource
                        commands.insert_resource(MyPlayerId(init_msg.id));

                        // Spawn all existing players (these are other players, not us)
                        for (id, player) in init_msg.other_players {
                            world::spawn_player(
                                &mut commands,
                                &mut meshes,
                                &mut materials,
                                id.0,
                                &player.pos,
                                false, // Other players are never local
                            );
                        }

                        // Spawn ourselves as the local player with position from server
                        world::spawn_player(
                            &mut commands,
                            &mut meshes,
                            &mut materials,
                            init_msg.id.0,
                            &init_msg.player.pos,
                            true, // This is us!
                        );
                    }
                    ServerMessage::Login(login_msg) => {
                        info!("Player {:?} logged in", login_msg.id);

                        // Login is always for another player (server doesn't send our own login back)
                        world::spawn_player(
                            &mut commands,
                            &mut meshes,
                            &mut materials,
                            login_msg.id.0,
                            &login_msg.player.pos,
                            false, // Never local
                        );
                    }
                    ServerMessage::Logoff(logoff_msg) => {
                        info!(
                            "Player {:?} logged off (graceful: {})",
                            logoff_msg.id, logoff_msg.graceful
                        );

                        // Find and despawn the entity with this PlayerId
                        for (entity, player_id) in player_query.iter() {
                            if *player_id == logoff_msg.id {
                                commands.entity(entity).despawn();
                                break;
                            }
                        }
                    }
                }
            }
            ServerToBevy::Disconnected => {
                error!("Disconnected from server");
                exit.send(AppExit::Success);
                return;
            }
        }
    }
}
