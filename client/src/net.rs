use common::net::MessageStream;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use quinn::{Connection, ConnectionError};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

// ============================================================================
// Network I/O Task
// ============================================================================

/// Message from network I/O task to Bevy main thread
#[derive(Debug, Clone)]
pub enum ServerToBevy {
    Message(ServerMessage),
    Disconnected,
}

/// Message from Bevy main thread to network I/O task
#[derive(Debug, Clone)]
pub enum BevyToServer {
    Send(ClientMessage),
    Close,
}

pub async fn network_io_task(
    connection: Connection,
    to_bevy: UnboundedSender<ServerToBevy>,
    mut from_bevy: UnboundedReceiver<BevyToServer>,
) {
    let stream = MessageStream::new(&connection);

    loop {
        tokio::select! {
            // Receive from server
            result = stream.recv::<ServerMessage>() => {
                match result {
                    Ok(msg) => {
                        if to_bevy.send(ServerToBevy::Message(msg)).is_err() {
                            // Bevy side closed, exit
                            break;
                        }
                    }
                    Err(e) => {
                        if let Some(conn_err) = e.downcast_ref::<ConnectionError>() {
                            match conn_err {
                                ConnectionError::ApplicationClosed { .. } => {
                                    eprintln!("Server closed the connection");
                                }
                                ConnectionError::TimedOut => {
                                    eprintln!("Server connection timed out");
                                }
                                ConnectionError::LocallyClosed => {
                                    eprintln!("Connection to server closed locally");
                                }
                                _ => {
                                    eprintln!("Connection error: {e}");
                                }
                            }
                        } else {
                            eprintln!("Error receiving message: {e}");
                        }
                        break;
                    }
                }
            }

            // Send to server
            cmd = from_bevy.recv() => {
                match cmd {
                    Some(BevyToServer::Send(msg)) => {
                        if let Err(e) = stream.send(&msg).await {
                            eprintln!("Error sending to server: {e}");
                            break;
                        }
                    }
                    Some(BevyToServer::Close) => {
                        connection.close(0u32.into(), b"client closing");
                        break;
                    }
                    None => {
                        // Bevy side closed, exit
                        break;
                    }
                }
            }
        }
    }

    // Notify Bevy that we're disconnected
    let _ = to_bevy.send(ServerToBevy::Disconnected);
}
