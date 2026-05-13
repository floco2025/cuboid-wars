use anyhow::Result;
use bincode::{Decode, Encode};
use quinn::Connection;

// ============================================================================
// Message Stream Abstraction
// ============================================================================
//
// Each message is sent on its own QUIC unidirectional stream. QUIC preserves
// ordering within one stream, not across streams, so the application protocol
// intentionally tolerates occasional cross-message reordering: snapshots are
// full-state with sequence guards, and edge-triggered cues are paired with
// snapshot fallback/idempotent handlers.

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
