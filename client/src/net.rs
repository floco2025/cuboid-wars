use bevy::app::AppExit;
use bevy::prelude::*;
use common::net::MessageStream;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use quinn::{Connection, ConnectionError};
use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender,
    error::{SendError, TryRecvError},
};
use crate::client::ClientState;

// ============================================================================
// Resources
// ============================================================================

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

pub fn server_to_bevy_system(
    mut game_state: ResMut<ClientState>,
    mut from_server: ResMut<ServerToBevyChannel>,
    mut exit: EventWriter<AppExit>,
) {
    // Process all available messages
    while let Ok(msg) = from_server.try_recv() {
        match msg {
            ServerToBevy::Message(server_msg) => {
                game_state.process_message(server_msg);
            }
            ServerToBevy::Disconnected => {
                error!("Disconnected from server");
                exit.send(AppExit::Success);
                return;
            }
        }
    }
}

