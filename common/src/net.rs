use anyhow::Result;
use bincode::{Decode, Encode};
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

    pub async fn send<T: Encode + Send + Sync>(&self, msg: &T) -> Result<()> {
        let mut stream = self.connection.open_uni().await?;
        let data = bincode::encode_to_vec(msg, bincode::config::standard())?;
        stream.write_all(&data).await?;
        stream.finish()?;
        Ok(())
    }

    pub async fn recv<T: Decode<()> + Send>(&self) -> Result<T> {
        let mut stream = self.connection.accept_uni().await?;
        let data = stream.read_to_end(1024 * 1024).await?; // 1MB limit
        let result = bincode::decode_from_slice(&data, bincode::config::standard())?.0;
        Ok(result)
    }
}
