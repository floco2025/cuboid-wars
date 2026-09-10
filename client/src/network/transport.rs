use anyhow::{Context, Result};
use bevy::prelude::{debug, error, trace};
use quinn::{ClientConfig, Connection, ConnectionError, RecvStream, SendStream};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use common::{
    config::{create_quinn_client_config, load_certs},
    network::{drive_lane, receive_lanes, send_message},
    protocol::*,
};

use super::impairment::{Impairment, impaired_receiver, impaired_sender};

// Message emitted by the network task toward the Bevy world. The `Message`
// variant carries the full `ServerMessage` (large; `SSnapshot` dominates),
// but these enum values are short-lived inside an mpsc queue and the extra
// padding on `Disconnected` is bounded by the queue depth — boxing here
// would force `Box::new` at every send + a deref at every recv with no
// real memory win.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ServerToClient {
    Message(ServerMessage),
    Disconnected,
}

// Message emitted by the Bevy world toward the network task.
#[derive(Debug, Clone)]
pub enum ClientToServer {
    Send(ClientMessage),
    // Crossing follow-up movement must stay behind its reliable boundary.
    SendReliable(ClientMessage),
    // Nothing sends this yet: today the client just exits and drops the
    // connection. Kept for an eventual graceful shutdown (e.g. an `AppExit`
    // hook) that tells the server why we left.
    Close,
}

impl ClientToServer {
    fn is_unreliable(&self) -> bool {
        matches!(self, Self::Send(message) if message.lane() == Lane::Unreliable)
    }
}

// Bidirectional bridge between the server connection and the Bevy world.
// `CLogin` is queued before this task runs, so the reliable lane opened here
// carries data at once and the server sees it immediately.
pub async fn network_io_task(
    connection: Connection,
    to_client: UnboundedSender<ServerToClient>,
    from_client: UnboundedReceiver<ClientToServer>,
    impairment: Impairment,
) {
    let to_client = impaired_sender(
        impairment,
        to_client,
        |event| matches!(event, ServerToClient::Message(message) if message.lane() == Lane::Unreliable),
    );
    let mut from_client = impaired_receiver(impairment, from_client, ClientToServer::is_unreliable);
    match connection.open_bi().await {
        Ok((send, recv)) => drive_lanes(&connection, send, recv, &to_client, &mut from_client, impairment).await,
        Err(error) => error!("failed to open the reliable lane: {error}"),
    }
    log_close_reason(&connection);
    debug!("network task exiting");
    let _ = to_client.send(ServerToClient::Disconnected);
}

async fn drive_lanes(
    connection: &Connection,
    send: SendStream,
    mut recv: RecvStream,
    to_client: &UnboundedSender<ServerToClient>,
    from_client: &mut UnboundedReceiver<ClientToServer>,
    impairment: Impairment,
) {
    let forward = |message: ServerMessage| {
        to_client
            .send(ServerToClient::Message(message))
            .context("client ingress channel closed")
    };
    tokio::join!(
        receive_lanes(connection, &mut recv, forward, || impairment.drops()),
        drive_lane(
            connection,
            "writer",
            write_outbound(connection, send, from_client, impairment)
        ),
    );
}

async fn write_outbound(
    connection: &Connection,
    mut send: SendStream,
    from_client: &mut UnboundedReceiver<ClientToServer>,
    impairment: Impairment,
) -> Result<()> {
    loop {
        let command = tokio::select! {
            command = from_client.recv() => command,
            _ = connection.closed() => return Ok(()),
        };
        let lane = if command.as_ref().is_some_and(ClientToServer::is_unreliable) {
            Lane::Unreliable
        } else {
            Lane::Reliable
        };
        match command {
            Some(ClientToServer::Send(message) | ClientToServer::SendReliable(message)) => {
                if lane == Lane::Unreliable && impairment.drops() {
                    continue;
                }
                trace!("sending to server: {:?}", message);
                send_message(connection, &mut send, lane, &message).await?;
            }
            Some(ClientToServer::Close) => {
                let _ = send.finish();
                connection.close(0u32.into(), b"client closing");
                return Ok(());
            }
            None => {
                debug!("client channel closed");
                let _ = send.finish();
                connection.close(0u32.into(), b"client closing");
                return Ok(());
            }
        }
    }
}

fn log_close_reason(connection: &Connection) {
    match connection.close_reason() {
        Some(ConnectionError::ApplicationClosed { .. }) => error!("server closed connection"),
        Some(ConnectionError::TimedOut) => error!("server connection timed out"),
        Some(ConnectionError::LocallyClosed) => debug!("connection to server closed locally"),
        Some(error) => error!("connection error: {error}"),
        None => debug!("disconnected from server"),
    }
}

// ============================================================================
// Connection Configuration
// ============================================================================

pub fn configure_client() -> Result<ClientConfig> {
    let certs = load_certs()?;

    let mut roots = rustls::RootCertStore::empty();
    for cert in certs {
        roots.add(cert).context("Failed to add certificate to root store")?;
    }

    let mut crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    crypto.alpn_protocols = vec![common::network::ALPN_PROTOCOL.to_vec()];

    create_quinn_client_config(crypto)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::{sync::mpsc::unbounded_channel, time::Instant};

    #[tokio::test(start_paused = true)]
    async fn crossing_followup_moves_share_reliable_order_and_fixed_delay() {
        let lag = Duration::from_millis(100);
        let impairment = Impairment {
            lag,
            jitter: 1.0,
            drop_probability: 1.0,
        };
        let (input, receiver) = unbounded_channel();
        let mut output = impaired_receiver(impairment, receiver, ClientToServer::is_unreliable);
        let movement = PlayerMovementState::new(Position::default(), PlayerMoveIntent::Idle, 0.0, 0.0);
        let start = Instant::now();
        input
            .send(ClientToServer::Send(ClientMessage::PortalCross(CPortalCross {
                seq: 1,
                entrance: movement,
                movement,
            })))
            .expect("transport input closed");
        for seq in 2..=10 {
            input
                .send(ClientToServer::SendReliable(ClientMessage::Move(CMove {
                    seq,
                    movement,
                })))
                .expect("transport input closed");
        }
        drop(input);
        for seq in 1..=10 {
            let command = output.recv().await.expect("reliable report missing");
            assert!(!command.is_unreliable());
            let actual = match command {
                ClientToServer::Send(ClientMessage::PortalCross(report)) => report.seq,
                ClientToServer::SendReliable(ClientMessage::Move(report)) => report.seq,
                _ => panic!("unexpected movement command"),
            };
            assert_eq!(actual, seq);
            assert_eq!(Instant::now() - start, lag);
        }
        assert!(ClientToServer::Send(ClientMessage::Move(CMove { seq: 11, movement })).is_unreliable());
    }
}
