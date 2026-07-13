use anyhow::{Context, Error, Result};
use bevy::prelude::{debug, error, trace};
use quinn::{ClientConfig, Connection, ConnectionError};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    time::{Duration, sleep},
};

use common::{
    config::{create_quinn_client_config, load_certs},
    network::MessageStream,
    protocol::*,
};

// Message emitted by the network task toward the Bevy world. The `Message`
// variant carries the full `ServerMessage` (large; `SSnapshot` dominates),
// but these enum values are short-lived inside an mpsc queue and the extra
// padding on `Disconnected` is bounded by the queue depth — boxing here
// would force `Box::new` at every send + a deref at every recv with no
// real memory win.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ServerToClient {
    Message(ServerMessage),
    Disconnected,
}

// Message emitted by the Bevy world toward the network task.
#[derive(Debug, Clone)]
pub enum ClientToServer {
    Send(ClientMessage),
    Close,
}

// Bidirectional bridge between the server connection and the Bevy world.
pub async fn network_io_task(
    connection: Connection,
    to_client: UnboundedSender<ServerToClient>,
    mut from_client: UnboundedReceiver<ClientToServer>,
    lag: Option<Duration>,
) {
    let stream = MessageStream::new(&connection);

    loop {
        tokio::select! {
            result = stream.recv::<ServerMessage>() => {
                if !handle_server_message(result, lag, &to_client) {
                    break;
                }
            }

            cmd = from_client.recv() => {
                if !handle_client_command(cmd, lag, &connection, &stream).await {
                    break;
                }
            }
        }
    }

    // Ensure disconnect notification is sent before task exits
    debug!("network task exiting");
    let _ = to_client.send(ServerToClient::Disconnected);
}

fn handle_server_message(
    result: Result<ServerMessage, Error>,
    lag: Option<Duration>,
    to_client: &UnboundedSender<ServerToClient>,
) -> bool {
    match result {
        Ok(msg) => {
            if let Some(delay) = lag {
                if to_client.is_closed() {
                    return false;
                }
                let sender = to_client.clone();
                tokio::spawn(async move {
                    sleep(delay).await;
                    let _ = sender.send(ServerToClient::Message(msg));
                });
                true
            } else {
                to_client.send(ServerToClient::Message(msg)).is_ok()
            }
        }
        Err(err) => {
            if let Some(conn_err) = err.downcast_ref::<ConnectionError>() {
                match conn_err {
                    ConnectionError::ApplicationClosed { .. } => {
                        error!("server closed connection");
                    }
                    ConnectionError::TimedOut => error!("server connection timed out"),
                    ConnectionError::LocallyClosed => {
                        debug!("connection to server closed locally");
                    }
                    _ => error!("connection error: {err}"),
                }
            } else {
                error!("error receiving from server: {err}");
            }
            false
        }
    }
}

async fn handle_client_command(
    cmd: Option<ClientToServer>,
    lag: Option<Duration>,
    connection: &Connection,
    stream: &MessageStream<'_>,
) -> bool {
    match cmd {
        Some(ClientToServer::Send(msg)) => {
            if let Some(delay) = lag {
                if connection.close_reason().is_some() {
                    return false;
                }
                let connection_clone = connection.clone();
                tokio::spawn(async move {
                    sleep(delay).await;
                    trace!("sending to server: {:?}", msg);
                    let stream = MessageStream::new(&connection_clone);
                    if let Err(e) = stream.send(&msg).await {
                        error!("error sending to server: {e}");
                        connection_clone.close(1u32.into(), b"send error");
                    }
                });
                true
            } else {
                trace!("sending to server: {:?}", msg);
                stream.send(&msg).await.map_or_else(
                    |e| {
                        error!("error sending to server: {e}");
                        false
                    },
                    |()| true,
                )
            }
        }
        Some(ClientToServer::Close) => {
            connection.close(0u32.into(), b"client closing");
            false
        }
        None => {
            debug!("client channel closed");
            false
        }
    }
}

// ============================================================================
// Connection Configuration
// ============================================================================

pub fn configure_client() -> Result<ClientConfig> {
    let certs = load_certs()?;

    let mut roots = rustls::RootCertStore::empty();
    for cert in certs {
        roots.add(cert).context("Failed to add certificate to root store")?;
    }

    let mut crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    crypto.alpn_protocols = vec![b"game".to_vec()];

    create_quinn_client_config(crypto)
}
