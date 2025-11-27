use bevy::prelude::{debug, error, trace};
use quinn::{Connection, ConnectionError};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

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
) {
    let stream = MessageStream::new(&connection);

    loop {
        tokio::select! {
            // Receive from server
            result = stream.recv::<ServerMessage>() => {
                match result {
                    Ok(msg) => {
                        if to_client.send(ServerToClient::Message(msg)).is_err() {
                            // Bevy side closed, exit
                            break;
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
                        trace!("sending to server: {:?}", msg);
                        if let Err(e) = stream.send(&msg).await {
                            error!("error sending to server: {e}");
                            break;
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
