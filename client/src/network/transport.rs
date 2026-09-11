use std::net::SocketAddr;

use anyhow::{Context, Result};
use bevy::prelude::{debug, error, trace};
use quinn::{ClientConfig, Connection, ConnectionError, Endpoint, RecvStream, SendStream};
use tokio::{
    runtime::Handle,
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
};

use common::{
    config::{create_quinn_client_config, load_certs},
    network::{drive_lane, receive_lanes, send_message},
    protocol::*,
};

use super::impairment::{Impairment, impaired_receiver, impaired_sender};

// Connects to `server` and starts the network task on the runtime; returns
// the app's ends of the two queues.
pub fn connect_to_server(
    handle: &Handle,
    server: SocketAddr,
    impairment: Impairment,
) -> Result<(UnboundedSender<ClientMessage>, UnboundedReceiver<ServerMessage>)> {
    let connection = handle.block_on(async {
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
        endpoint.set_default_client_config(configure_client()?);
        endpoint
            .connect(server, "localhost")?
            .await
            .context("failed to connect to server")
    })?;
    let (to_client, from_server) = unbounded_channel();
    let (to_server, from_client) = unbounded_channel();
    handle.spawn(network_io_task(connection, to_client, from_client, impairment));
    Ok((to_server, from_server))
}

// Bidirectional bridge between the server connection and the Bevy world.
// `CLogin` is queued before this task runs, so the reliable lane opened here
// carries data at once and the server sees it immediately. Returning drops
// `to_client`, which is how the app learns the connection is gone.
async fn network_io_task(
    connection: Connection,
    to_client: UnboundedSender<ServerMessage>,
    from_client: UnboundedReceiver<ClientMessage>,
    impairment: Impairment,
) {
    let to_client = impaired_sender(impairment, to_client, |message: &ServerMessage| {
        message.lane() == Lane::Unreliable
    });
    let mut from_client = impaired_receiver(impairment, from_client, |message: &ClientMessage| {
        message.lane() == Lane::Unreliable
    });
    match connection.open_bi().await {
        Ok((send, recv)) => drive_lanes(&connection, send, recv, &to_client, &mut from_client, impairment).await,
        Err(error) => error!("failed to open the reliable lane: {error}"),
    }
    log_close_reason(&connection);
    debug!("network task exiting");
}

async fn drive_lanes(
    connection: &Connection,
    send: SendStream,
    mut recv: RecvStream,
    to_client: &UnboundedSender<ServerMessage>,
    from_client: &mut UnboundedReceiver<ClientMessage>,
    impairment: Impairment,
) {
    let forward = |message: ServerMessage| to_client.send(message).context("client ingress channel closed");
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
    from_client: &mut UnboundedReceiver<ClientMessage>,
    impairment: Impairment,
) -> Result<()> {
    loop {
        let message = tokio::select! {
            message = from_client.recv() => message,
            _ = connection.closed() => return Ok(()),
        };
        let Some(message) = message else {
            debug!("client hung up");
            let _ = send.finish();
            connection.close(0u32.into(), b"client closing");
            return Ok(());
        };
        let lane = message.lane();
        if lane == Lane::Unreliable && impairment.drops() {
            continue;
        }
        trace!("sending to server: {:?}", message);
        send_message(connection, &mut send, lane, &message).await?;
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

fn configure_client() -> Result<ClientConfig> {
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
