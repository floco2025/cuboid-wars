use anyhow::Error;
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
    while let Some(incoming) = endpoint.accept().await {
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
                        error!("failed to register channel for {:?}", id);
                        return;
                    }

                    // Run per-client network I/O task for the new client
                    per_client_network_io_task(id, connection, to_server_clone, from_server).await;
                }
                Err(e) => {
                    error!("failed to establish connection: {e}");
                }
            }
        });
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
            result = stream.recv::<ClientMessage>() => {
                if !handle_client_message(id, result, &to_server) {
                    break;
                }
            }

            cmd = from_server.recv() => {
                if !handle_server_command(id, cmd, &connection, &stream).await {
                    break;
                }
            }
        }
    }

    // Ensure disconnect notification is sent before task exits
    debug!("{:?} network task exiting", id);
    let _ = to_server.send((id, ClientToServer::Disconnected));
}

fn handle_client_message(
    id: PlayerId,
    result: Result<ClientMessage, Error>,
    to_server: &UnboundedSender<(PlayerId, ClientToServer)>,
) -> bool {
    match result {
        Ok(msg) => {
            trace!("received from {:?}: {:?}", id, msg);
            to_server
                .send((id, ClientToServer::Message(msg)))
                .map_err(|e| error!("error sending to main task: {e}"))
                .is_ok()
        }
        Err(err) => {
            if let Some(conn_err) = err.downcast_ref::<ConnectionError>() {
                match conn_err {
                    ConnectionError::ApplicationClosed { .. } => debug!("{:?} closed connection", id),
                    ConnectionError::TimedOut => debug!("{:?} timed out", id),
                    ConnectionError::LocallyClosed => debug!("{:?} locally closed", id),
                    _ => error!("connection error for {:?}: {err}", id),
                }
            } else {
                error!("error receiving from {:?}: {err}", id);
            }
            false
        }
    }
}

async fn handle_server_command(
    id: PlayerId,
    cmd: Option<ServerToClient>,
    connection: &Connection,
    stream: &MessageStream<'_>,
) -> bool {
    match cmd {
        Some(ServerToClient::Send(msg)) => {
            trace!("sending to {:?}: {:?}", id, msg);
            stream
                .send(&msg)
                .await
                .map_err(|e| warn!("error sending to {:?}: {e}", id))
                .is_ok()
        }
        Some(ServerToClient::Close) => {
            debug!("closing connection to {:?}", id);
            connection.close(0u32.into(), b"server closing");
            false
        }
        None => {
            debug!("server channel closed for {:?}", id);
            false
        }
    }
}
