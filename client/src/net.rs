use anyhow::Result;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use quinn::{Connection, ConnectionError};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

#[cfg(feature = "json")]
use serde::{de::DeserializeOwned, Serialize};

#[cfg(feature = "bincode")]
use bincode::{Decode, Encode};

// ============================================================================
// Message Stream Abstraction
// ============================================================================

pub struct MessageStream<'a> {
    connection: &'a Connection,
}

impl<'a> MessageStream<'a> {
    #[must_use]
    pub const fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    #[cfg(feature = "json")]
    pub async fn send<T: Serialize + Send + Sync>(&self, msg: &T) -> Result<()> {
        let mut stream = self.connection.open_uni().await?;
        let data = serde_json::to_vec(msg)?;
        stream.write_all(&data).await?;
        stream.finish()?;
        Ok(())
    }

    #[cfg(feature = "bincode")]
    pub async fn send<T: Encode + Send + Sync>(&self, msg: &T) -> Result<()> {
        let mut stream = self.connection.open_uni().await?;
        let data = bincode::encode_to_vec(msg, bincode::config::standard())?;
        stream.write_all(&data).await?;
        stream.finish()?;
        Ok(())
    }

    #[cfg(feature = "json")]
    pub async fn recv<T: DeserializeOwned + Send>(&self) -> Result<T> {
        let mut stream = self.connection.accept_uni().await?;
        let data = stream.read_to_end(1024 * 1024).await?; // 1MB limit
        let result = serde_json::from_slice(&data)?;
        Ok(result)
    }

    #[cfg(feature = "bincode")]
    pub async fn recv<T: Decode<()> + Send>(&self) -> Result<T> {
        let mut stream = self.connection.accept_uni().await?;
        let data = stream.read_to_end(1024 * 1024).await?; // 1MB limit
        let result = bincode::decode_from_slice(&data, bincode::config::standard())?.0;
        Ok(result)
    }
}

// ============================================================================
// Channel Messages
// ============================================================================

/// Message from network I/O task to Bevy main thread
#[derive(Debug, Clone)]
pub enum ServerToClient {
    Message(ServerMessage),
    Disconnected,
}

/// Message from Bevy main thread to network I/O task
#[derive(Debug, Clone)]
pub enum ClientToServer {
    Send(ClientMessage),
    Close,
}

// ============================================================================
// Network I/O Task
// ============================================================================

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
            cmd = from_client.recv() => {
                match cmd {
                    Some(ClientToServer::Send(msg)) => {
                        if let Err(e) = stream.send(&msg).await {
                            eprintln!("Error sending to server: {e}");
                            break;
                        }
                    }
                    Some(ClientToServer::Close) => {
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
    let _ = to_client.send(ServerToClient::Disconnected);
}
