use bevy::prelude::{debug, error, trace};
use quinn::{Connection, ConnectionError};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    time::{Duration, sleep},
};

use common::net::MessageStream;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;

// ============================================================================
// Network I/O Task
// ============================================================================

// Message from network I/O task to client main thread
#[derive(Debug, Clone)]
pub enum ServerToClient {
    Message(ServerMessage),
    Disconnected,
}

// Message from client main thread to network I/O task
#[derive(Debug, Clone)]
pub enum ClientToServer {
    Send(ClientMessage),
    Close,
}

pub async fn network_io_task(
    connection: Connection,
    to_client: UnboundedSender<ServerToClient>,
    mut from_client: UnboundedReceiver<ClientToServer>,
    lag_ms: u64,
) {
    let stream = MessageStream::new(&connection);

    loop {
        tokio::select! {
            // Receive from server
            result = stream.recv::<ServerMessage>() => {
                match result {
                    Ok(msg) => {
                        if lag_ms > 0 {
                            // Check if channel is still open before spawning
                            if to_client.is_closed() {
                                break;
                            }
                            let to_client_clone = to_client.clone();
                            tokio::spawn(async move {
                                sleep(Duration::from_millis(lag_ms)).await;
                                let _ = to_client_clone.send(ServerToClient::Message(msg));
                            });
                        } else {
                            if to_client.send(ServerToClient::Message(msg)).is_err() {
                                // Bevy side closed, exit
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        if let Some(conn_err) = e.downcast_ref::<ConnectionError>() {
                            match conn_err {
                                ConnectionError::ApplicationClosed { .. } => {
                                    error!("server closed connection");
                                }
                                ConnectionError::TimedOut => {
                                    error!("server connection timed out");
                                }
                                ConnectionError::LocallyClosed => {
                                    debug!("connection to server closed locally");
                                }
                                _ => {
                                    error!("connection error: {}", e);
                                }
                            }
                        } else {
                            error!("error receiving from server: {e}");
                        }
                        break;
                    }
                }
            }

            // Send to server
            cmd = from_client.recv() => {
                match cmd {
                    Some(ClientToServer::Send(msg)) => {
                        if lag_ms > 0 {
                            // Check if connection is still open before spawning
                            if connection.close_reason().is_some() {
                                break;
                            }
                            let connection_clone = connection.clone();
                            tokio::spawn(async move {
                                sleep(Duration::from_millis(lag_ms)).await;
                                trace!("sending to server: {:?}", msg);
                                let stream = MessageStream::new(&connection_clone);
                                if let Err(e) = stream.send(&msg).await {
                                    error!("error sending to server: {e}");
                                    // Close connection on error to trigger cleanup
                                    connection_clone.close(1u32.into(), b"send error");
                                }
                            });
                        } else {
                            trace!("sending to server: {:?}", msg);
                            if let Err(e) = stream.send(&msg).await {
                                error!("error sending to server: {e}");
                                break;
                            }
                        }
                    }
                    Some(ClientToServer::Close) => {
                        connection.close(0u32.into(), b"client closing");
                        break;
                    }
                    None => {
                        debug!("client channel closed");
                        break;
                    }
                }
            }
        }
    }

    // Ensure disconnect notification is sent before task exits
    debug!("network task exiting");
    let _ = to_client.send(ServerToClient::Disconnected);
}
