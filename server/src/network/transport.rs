use anyhow::Error;
use bevy::prelude::*;
use quinn::{Connection, ConnectionError, Endpoint};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use common::{network::MessageStream, protocol::*};

// ============================================================================
// Accept Connections Task
// ============================================================================

// Task to accept incoming connections and spawn per-client network I/O tasks.
//
// Registrations and client messages flow through the SAME channel
// (`to_server`) so they're strictly ordered: a per-client task only starts
// recv'ing after its `Registration` is enqueued, guaranteeing the main loop
// sees the registration before any of that client's messages. A separate
// channel here would race — see the long-standing "non-login message before
// authenticating" bug.
pub async fn accept_connections_task(endpoint: Endpoint, to_server: UnboundedSender<(PlayerId, ClientToServer)>) {
    let mut next_player_id = 1u32;
    while let Some(incoming) = endpoint.accept().await {
        let id = PlayerId(next_player_id);
        next_player_id = next_player_id
            .checked_add(1)
            .expect("player ID overflow: 4 billion players connected!");

        let to_server_clone = to_server.clone();
        tokio::spawn(async move {
            match incoming.await {
                Ok(connection) => {
                    info!("player {:?} connection established", id);
                    let (to_client, from_server) = unbounded_channel();

                    if to_server_clone
                        .send((id, ClientToServer::Registration { to_client }))
                        .is_err()
                    {
                        error!("failed to register {:?}", id);
                        return;
                    }

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

// Message from per client network I/O task to server for existing clients.
// `Registration` is sent once by `accept_connections_task` before any
// `Message` from the same player; sharing the channel keeps that ordering.
#[derive(Debug)]
pub enum ClientToServer {
    Registration { to_client: UnboundedSender<ServerToClient> },
    Message(ClientMessage),
    Disconnected,
}

// Message from server to per client network I/O task. `Send` carries the
// full `ServerMessage` (large; `SSnapshot` dominates), but values live
// briefly inside an mpsc queue and the `Close` padding is bounded by the
// queue depth — boxing here would force `Box::new` at every send + a deref
// at every recv with no real memory win.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
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
