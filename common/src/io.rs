use anyhow::Result;
use quinn::Connection;

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

    pub async fn send<T: serde::Serialize + Send + Sync>(&self, msg: &T) -> Result<()> {
        let mut stream = self.connection.open_uni().await?;

        #[cfg(feature = "json")]
        let data = serde_json::to_vec(msg)?;

        #[cfg(feature = "bincode")]
        let data = bincode::serialize(msg)?;

        stream.write_all(&data).await?;
        stream.finish()?;
        Ok(())
    }

    pub async fn recv<T: serde::de::DeserializeOwned + Send>(&self) -> Result<T> {
        let mut recv = self.connection.accept_uni().await?;
        let data = recv.read_to_end(1024 * 1024).await?; // 1MB limit

        #[cfg(feature = "json")]
        let result = serde_json::from_slice(&data)?;

        #[cfg(feature = "bincode")]
        let result = bincode::deserialize(&data)?;

        Ok(result)
    }
}
