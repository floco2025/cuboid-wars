#[allow(clippy::wildcard_imports)]
use bevy::log::*;
use quinn::{Connection, ConnectionError, Endpoint};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use common::net::MessageStream;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;

// ============================================================================
// Accept Connections Task
// ============================================================================

// Task to accept incoming connections and spawn per-client network I/O tasks
pub async fn accept_connections_task(
    endpoint: Endpoint,
    to_server_from_accept: UnboundedSender<(PlayerId, UnboundedSender<ServerToClient>)>,
    to_server: UnboundedSender<(PlayerId, ClientToServer)>,
) {
    let mut next_player_id = 1u32;
    loop {
        if let Some(incoming) = endpoint.accept().await {
            // Generate ID
            let id = PlayerId(next_player_id);
            next_player_id = next_player_id
                .checked_add(1)
                .expect("player ID overflow: 4 billion players connected!");

            // Spawn per client network I/O task
            let to_server_from_accept_clone = to_server_from_accept.clone();
            let to_server_clone = to_server.clone();
            tokio::spawn(async move {
                match incoming.await {
                    Ok(connection) => {
                        info!("player {:?} connection established", id);

                        // New channel for sending from the server to the new client network IO task
                        let (to_client, from_server) = unbounded_channel();

                        // Send the server the new channel for sending to the new client network IO
                        // task, so that the server can send messages to the new client.
                        if to_server_from_accept_clone.send((id, to_client)).is_err() {
                            error!("failed to send NewClientToServer for {:?}", id);
                            return;
                        }

                        // Run per-client network I/O task for the new client
                        per_client_network_io_task(id, connection, to_server_clone, from_server).await;
                    }
                    Err(e) => {
                        error!("failed to establish connection: {}", e);
                    }
                }
            });
        }
    }
}

// ============================================================================
// Per Client Network I/O Task
// ============================================================================

// Message from per client network I/O task to server for existing clients
#[derive(Debug)]
pub enum ClientToServer {
    Message(ClientMessage),
    Disconnected,
}

// Message from server to per client network I/O task
#[derive(Debug)]
pub enum ServerToClient {
    Send(ServerMessage),
    Close,
}

pub async fn per_client_network_io_task(
    id: PlayerId,
    connection: Connection,
    to_server: UnboundedSender<(PlayerId, ClientToServer)>,
    mut from_server: UnboundedReceiver<ServerToClient>,
) {
    let stream = MessageStream::new(&connection);

    loop {
        tokio::select! {
            // Receive from client
            result = stream.recv() => {
                match result {
                    Ok(msg) => {
                        trace!("received from {:?}: {:?}", id, msg);
                        if let Err(e) = to_server.send((id, ClientToServer::Message(msg))) {
                            error!("error sending to main task: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        if let Some(conn_err) = e.downcast_ref::<ConnectionError>() {
                            match conn_err {
                                ConnectionError::ApplicationClosed { .. } => {
                                    debug!("{:?} closed connection", id);
                                }
                                ConnectionError::TimedOut => {
                                    debug!("{:?} timed out", id);
                                }
                                ConnectionError::LocallyClosed => {
                                    debug!("{:?} locally closed", id);
                                }
                                _ => {
                                    error!("connection error for {:?}: {}", id, e);
                                }
                            }
                        } else {
                            error!("error receiving from {:?}: {}", id, e);
                        }
                        break;
                    }
                }
            }

            // Send to client
            cmd = from_server.recv() => {
                match cmd {
                    Some(ServerToClient::Send(msg)) => {
                        trace!("sending to {:?}: {:?}", id, msg);
                        if let Err(e) = stream.send(&msg).await {
                            warn!("error sending to {:?}: {}", id, e);
                            break;
                        }
                    }
                    Some(ServerToClient::Close) => {
                        debug!("closing connection to {:?}", id);
                        connection.close(0u32.into(), b"server closing");
                        break;
                    }
                    None => {
                        debug!("server channel closed for {:?}", id);
                        break;
                    }
                }
            }
        }
    }

    // Ensure disconnect notification is sent before task exits
    debug!("{:?} network task exiting", id);
    let _ = to_server.send((id, ClientToServer::Disconnected));
}
