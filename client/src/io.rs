use crate::GameClient;
use anyhow::Result;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use quinn::{Connection, ConnectionError};
use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(feature = "json")]
use serde::{Serialize, de::DeserializeOwned};

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
// Network I/O
// ============================================================================

pub async fn send_message(connection: &Connection, msg: &ClientMessage) -> Result<()> {
    let stream = MessageStream::new(connection);
    stream.send(msg).await
}

pub async fn receive_messages(connection: Arc<Connection>, client: Arc<Mutex<GameClient>>) {
    let stream = MessageStream::new(&connection);

    loop {
        match stream.recv::<ServerMessage>().await {
            Ok(msg) => {
                let mut client_guard = client.lock().await;
                client_guard.process_message(msg);
            }
            Err(e) => {
                if let Some(conn_err) = e.downcast_ref::<ConnectionError>() {
                    match conn_err {
                        ConnectionError::ApplicationClosed { .. } => {
                            println!("Server closed the connection");
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
                return;
            }
        }
    }
}
