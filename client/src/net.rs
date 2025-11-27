use bevy::prelude::{debug, error, trace};
use quinn::{Connection, ConnectionError};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError},
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

    'outer: loop {
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
                // Send first message and all queued messages
                let mut cmd = cmd;
                loop {
                    match cmd {
                        Some(ClientToServer::Send(msg)) => {
                            trace!("sending to server: {:?}", msg);
                            if let Err(e) = stream.send(&msg).await {
                                error!("error sending to server: {e}");
                                break 'outer;
                            }
                        }
                        Some(ClientToServer::Close) => {
                            connection.close(0u32.into(), b"client closing");
                            break 'outer;
                        }
                        None => {
                            debug!("client channel closed");
                            break 'outer;
                        }
                    }

                    // Try to get more messages
                    match from_client.try_recv() {
                        Ok(new_cmd) => cmd = Some(new_cmd),
                        Err(TryRecvError::Empty) => {
                            break; // No more messages
                        }
                        Err(TryRecvError::Disconnected) => {
                            debug!("client channel closed");
                            break 'outer;
                        }
                    }
                }

                // Apply lag after sending batch
                if lag_ms > 0 {
                    sleep(Duration::from_millis(lag_ms)).await;
                }
            }
        }
    }

    // Ensure disconnect notification is sent before task exits
    debug!("network task exiting");
    let _ = to_client.send(ServerToClient::Disconnected);
}
