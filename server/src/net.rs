use bevy::log::{debug, error, trace, warn};
use quinn::{Connection, ConnectionError};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use common::net::MessageStream;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;

// ============================================================================
// Per Client Network I/O Task
// ============================================================================

// Message from per client network I/O task to server
#[derive(Debug)]
pub enum ClientToServer {
    Connected(UnboundedSender<ServerToClient>),
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
            result = stream.recv::<ClientMessage>() => {
                match result {
                    Ok(msg) => {
                        trace!("received from client {:?}: {:?}", id, msg);
                        if let Err(e) = to_server.send((id, ClientToServer::Message(msg))) {
                            error!("error sending to main task: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        if let Some(conn_err) = e.downcast_ref::<ConnectionError>() {
                            match conn_err {
                                ConnectionError::ApplicationClosed { .. } => {
                                    debug!("client {:?} closed connection", id);
                                }
                                ConnectionError::TimedOut => {
                                    debug!("client {:?} timed out", id);
                                }
                                ConnectionError::LocallyClosed => {
                                    debug!("client {:?} locally closed", id);
                                }
                                _ => {
                                    error!("connection error for client {:?}: {}", id, e);
                                }
                            }
                        } else {
                            error!("error receiving from client {:?}: {}", id, e);
                        }
                        break;
                    }
                }
            }

            // Send to client
            cmd = from_server.recv() => {
                match cmd {
                    Some(ServerToClient::Send(msg)) => {
                        trace!("sending to client {:?}: {:?}", id, msg);
                        if let Err(e) = stream.send(&msg).await {
                            warn!("error sending to client {:?}: {}", id, e);
                            break;
                        }
                    }
                    Some(ServerToClient::Close) => {
                        debug!("closing connection to client {:?}", id);
                        connection.close(0u32.into(), b"server closing");
                        break;
                    }
                    None => {
                        debug!("server channel closed for client {:?}", id);
                        break;
                    }
                }
            }
        }
    }

    // Ensure disconnect notification is sent before task exits
    debug!("client {:?} network task exiting", id);
    let _ = to_server.send((id, ClientToServer::Disconnected));
}
