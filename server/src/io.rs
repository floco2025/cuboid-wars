#[allow(clippy::wildcard_imports)]
use common::{io::MessageStream, protocol::*};
use quinn::{Connection, ConnectionError};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{debug, error, instrument, trace, warn};

// ============================================================================
// Client I/O Task
// ============================================================================

// Message from client I/O task to server
#[derive(Debug)]
pub enum ClientToServer {
    Message(ClientMessage),
    Disconnected,
}

// Message from server to client I/O task
#[derive(Debug)]
pub enum ServerToClient {
    Send(ServerMessage),
    Close,
}

#[instrument(skip(connection, to_server, from_server))]
pub async fn client_io_task(
    id: u32,
    connection: Connection,
    to_server: UnboundedSender<(u32, ClientToServer)>,
    mut from_server: UnboundedReceiver<ServerToClient>,
) {
    let stream = MessageStream::new(&connection);

    loop {
        tokio::select! {
            // Receive from client
            result = stream.recv::<ClientMessage>() => {
                match result {
                    Ok(msg) => {
                        trace!(msg = ?msg, "received message from client");
                        if let Err(e) = to_server.send((id, ClientToServer::Message(msg))) {
                            error!("error sending to main task: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        if let Some(conn_err) = e.downcast_ref::<ConnectionError>() {
                            match conn_err {
                                ConnectionError::ApplicationClosed { .. } => {
                                    debug!("client closed connection");
                                }
                                ConnectionError::TimedOut => {
                                    debug!("client connection timed out");
                                }
                                ConnectionError::LocallyClosed => {
                                    debug!("connection to client closed locally");
                                }
                                _ => {
                                    error!("connection error: {}", e);
                                }
                            }
                        } else {
                            error!("error receiving from client: {}", e);
                        }
                        break;
                    }
                }
            }

            // Send to client
            cmd = from_server.recv() => {
                match cmd {
                    Some(ServerToClient::Send(msg)) => {
                        if let Err(e) = stream.send(&msg).await {
                            warn!("error sending to client: {}", e);
                            break;
                        }
                        trace!(msg = ?msg, "sent message to client");
                    }
                    Some(ServerToClient::Close) => {
                        debug!("received close command");
                        connection.close(0u32.into(), b"server closing");
                        break;
                    }
                    None => {
                        debug!("server send channel closed");
                        break;
                    }
                }
            }
        }
    }

    // Ensure disconnect notification is sent before task exits
    debug!("I/O task ending, sending disconnect notification");
    let _ = to_server.send((id, ClientToServer::Disconnected));
}
