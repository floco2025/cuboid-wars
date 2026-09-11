use std::net::SocketAddr;

use anyhow::{Context, Result};
use bevy::prelude::*;
use quinn::{Connection, ConnectionError, Endpoint, RecvStream, SendStream};
use tokio::{
    runtime::Handle,
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
};

use common::{
    network::{drive_lane, receive_lanes, send_message},
    protocol::*,
};

use super::resources::ClientLink;
use crate::config::configure_server;

// Binds the QUIC endpoint and accepts connections on the runtime; each one
// registers its `ClientLink` through `register`.
pub fn listen(handle: &Handle, bind: SocketAddr, register: UnboundedSender<ClientLink>) -> Result<()> {
    let _runtime = handle.enter();
    let endpoint = Endpoint::server(configure_server()?, bind)?;
    println!("quic server listening on {bind}");
    handle.spawn(accept_connections_task(endpoint, register));
    Ok(())
}

// A client's link is registered before its task reads anything, so the
// ingress owns the receiver before any of that client's messages exist. That
// ordering used to depend on registrations and messages sharing one queue
// (the "non-login message before authenticating" bug); per-link receivers
// make it structural.
async fn accept_connections_task(endpoint: Endpoint, register: UnboundedSender<ClientLink>) {
    while let Some(incoming) = endpoint.accept().await {
        let register = register.clone();
        tokio::spawn(async move {
            match incoming.await {
                Ok(connection) => {
                    let peer = connection.remote_address();
                    info!("{peer} connected");
                    let (to_client, from_server) = unbounded_channel();
                    let (to_server, from_client) = unbounded_channel();
                    if register.send(ClientLink { to_client, from_client }).is_err() {
                        error!("server ingress closed; dropping {peer}");
                        return;
                    }
                    per_client_network_io_task(connection, to_server, from_server).await;
                }
                Err(e) => {
                    error!("failed to establish connection: {e}");
                }
            }
        });
    }
}

// The client opens the reliable lane and writes `CLogin` on it straight away,
// so accepting that stream is the whole handshake. Returning drops
// `to_server`, which is how the ingress learns the client is gone.
async fn per_client_network_io_task(
    connection: Connection,
    to_server: UnboundedSender<ClientMessage>,
    mut from_server: UnboundedReceiver<ServerMessage>,
) {
    let peer = connection.remote_address();
    match connection.accept_bi().await {
        Ok((send, recv)) => drive_lanes(&connection, send, recv, &to_server, &mut from_server).await,
        Err(error) => debug!("{peer} closed before opening the reliable lane: {error}"),
    }
    log_close_reason(&connection);
    debug!("{peer} network task exiting");
}

async fn drive_lanes(
    connection: &Connection,
    send: SendStream,
    mut recv: RecvStream,
    to_server: &UnboundedSender<ClientMessage>,
    from_server: &mut UnboundedReceiver<ServerMessage>,
) {
    let peer = connection.remote_address();
    let forward = |message: ClientMessage| {
        trace!("received from {peer}: {message:?}");
        to_server.send(message).context("server ingress channel closed")
    };
    tokio::join!(
        receive_lanes(connection, &mut recv, forward, || false),
        drive_lane(connection, "writer", write_outbound(connection, send, from_server)),
    );
}

async fn write_outbound(
    connection: &Connection,
    mut send: SendStream,
    from_server: &mut UnboundedReceiver<ServerMessage>,
) -> Result<()> {
    let peer = connection.remote_address();
    loop {
        let message = tokio::select! {
            message = from_server.recv() => message,
            _ = connection.closed() => return Ok(()),
        };
        let Some(message) = message else {
            debug!("server hung up on {peer}");
            let _ = send.finish();
            connection.close(0u32.into(), b"server closing");
            return Ok(());
        };
        trace!("sending to {peer}: {message:?}");
        send_message(connection, &mut send, message.lane(), &message).await?;
    }
}

fn log_close_reason(connection: &Connection) {
    let peer = connection.remote_address();
    match connection.close_reason() {
        Some(ConnectionError::ApplicationClosed { .. }) => debug!("{peer} closed connection"),
        Some(ConnectionError::TimedOut) => debug!("{peer} timed out"),
        Some(ConnectionError::LocallyClosed) => debug!("{peer} locally closed"),
        Some(error) => error!("connection error for {peer}: {error}"),
        None => debug!("{peer} disconnected"),
    }
}
