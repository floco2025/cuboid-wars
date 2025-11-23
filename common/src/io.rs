use anyhow::Result;
use quinn::Connection;

#[cfg(feature = "json")]
use serde::{Deserialize, Serialize};

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
        let mut recv = self.connection.accept_uni().await?;
        let data = recv.read_to_end(1024 * 1024).await?; // 1MB limit
        let result = serde_json::from_slice(&data)?;
        Ok(result)
    }

    #[cfg(feature = "bincode")]
    pub async fn recv<T: Decode<()> + Send>(&self) -> Result<T> {
        let mut recv = self.connection.accept_uni().await?;
        let data = recv.read_to_end(1024 * 1024).await?; // 1MB limit
        let result = bincode::decode_from_slice(&data, bincode::config::standard())?.0;
        Ok(result)
    }
}
