use anyhow::{Context, Result};
use bevy::prelude::*;
use quinn::{Connection, ConnectionError, Endpoint, RecvStream, SendStream};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use common::{
    network::{drive_lane, receive_lanes, send_message},
    protocol::*,
};

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
                    info!("player#{} connection established", id.0);
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

// The client opens the reliable lane and writes `CLogin` on it straight away,
// so accepting that stream is the whole handshake.
pub async fn per_client_network_io_task(
    id: PlayerId,
    connection: Connection,
    to_server: UnboundedSender<(PlayerId, ClientToServer)>,
    mut from_server: UnboundedReceiver<ServerToClient>,
) {
    match connection.accept_bi().await {
        Ok((send, recv)) => drive_lanes(id, &connection, send, recv, &to_server, &mut from_server).await,
        Err(error) => debug!("player#{} closed before opening the reliable lane: {error}", id.0),
    }
    log_close_reason(id, &connection);
    debug!("player#{} network task exiting", id.0);
    let _ = to_server.send((id, ClientToServer::Disconnected));
}

async fn drive_lanes(
    id: PlayerId,
    connection: &Connection,
    send: SendStream,
    mut recv: RecvStream,
    to_server: &UnboundedSender<(PlayerId, ClientToServer)>,
    from_server: &mut UnboundedReceiver<ServerToClient>,
) {
    let forward = |message: ClientMessage| {
        trace!("received from {:?}: {:?}", id, message);
        to_server
            .send((id, ClientToServer::Message(message)))
            .context("server ingress channel closed")
    };
    tokio::join!(
        receive_lanes(connection, &mut recv, forward, || false),
        drive_lane(connection, "writer", write_outbound(id, connection, send, from_server)),
    );
}

async fn write_outbound(
    id: PlayerId,
    connection: &Connection,
    mut send: SendStream,
    from_server: &mut UnboundedReceiver<ServerToClient>,
) -> Result<()> {
    loop {
        let command = tokio::select! {
            command = from_server.recv() => command,
            _ = connection.closed() => return Ok(()),
        };
        match command {
            Some(ServerToClient::Send(message)) => {
                trace!("sending to {:?}: {:?}", id, message);
                send_message(connection, &mut send, message.lane(), &message).await?;
            }
            Some(ServerToClient::Close) => {
                debug!("closing connection to player#{}", id.0);
                let _ = send.finish();
                connection.close(0u32.into(), b"server closing");
                return Ok(());
            }
            None => {
                debug!("server channel closed for {:?}", id);
                let _ = send.finish();
                connection.close(0u32.into(), b"server closing");
                return Ok(());
            }
        }
    }
}

fn log_close_reason(id: PlayerId, connection: &Connection) {
    match connection.close_reason() {
        Some(ConnectionError::ApplicationClosed { .. }) => debug!("{:?} closed connection", id),
        Some(ConnectionError::TimedOut) => debug!("{:?} timed out", id),
        Some(ConnectionError::LocallyClosed) => debug!("{:?} locally closed", id),
        Some(error) => error!("connection error for {:?}: {error}", id),
        None => debug!("{:?} disconnected", id),
    }
}
