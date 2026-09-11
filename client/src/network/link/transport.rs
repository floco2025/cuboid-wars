use std::{
    net::{SocketAddr, UdpSocket},
    ops::ControlFlow,
    time::Instant,
};

use anyhow::{Context, Result, anyhow, bail};
use bevy::prelude::{debug, error, warn};
use bincode::Decode;
use crossbeam_channel::{Receiver, Sender, unbounded};
use renet::RenetClient;
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};

use common::{
    network::{CHANNELS, PROTOCOL_ID, channel_for, connection_config, decode_message, encode_message, unix_now},
    protocol::*,
};

use super::{
    impairment::{DelayQueue, Impairment},
    resources::ServerLink,
};

// A joined game's connection, pumped from the app: `receive` in the network
// set, `flush` at the end of the frame.
pub struct RemoteLink {
    client: RenetClient,
    transport: NetcodeClientTransport,
    outgoing: Receiver<ClientMessage>,
    impairment: Impairment,
    inbound: DelayQueue<ServerMessage>,
    outbound: DelayQueue<ClientMessage>,
    polled: Instant,
}

// Starts connecting to `server`; returns the app's sender and the link.
pub fn connect(server: SocketAddr, impairment: Impairment) -> Result<(Sender<ClientMessage>, ServerLink)> {
    let bind: SocketAddr = if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }.parse()?;
    let socket = UdpSocket::bind(bind).context("failed to open a UDP socket")?;
    let authentication = ClientAuthentication::Unsecure {
        protocol_id: PROTOCOL_ID,
        client_id: rand::random(),
        server_addr: server,
        user_data: None,
    };
    let transport = NetcodeClientTransport::new(unix_now(), authentication, socket)
        .map_err(|error| anyhow!("failed to start the connection: {error:?}"))?;
    let (to_server, outgoing) = unbounded();
    let link = RemoteLink {
        client: RenetClient::new(connection_config()),
        transport,
        outgoing,
        impairment,
        inbound: DelayQueue::default(),
        outbound: DelayQueue::default(),
        polled: Instant::now(),
    };
    Ok((to_server, ServerLink::Remote(Box::new(link))))
}

impl RemoteLink {
    // Advances the connection by the wall time since the last poll, reads
    // every waiting packet, and hands the messages that are due to `sink`
    // until it breaks; the rest stay queued. An error is the link closing,
    // with the reason. Without simulated lag the delay queues stay empty and
    // messages pass straight through.
    pub fn receive(&mut self, now: Instant, mut sink: impl FnMut(ServerMessage) -> ControlFlow<()>) -> Result<()> {
        let delta = now.saturating_duration_since(self.polled);
        self.polled = now;
        self.client.update(delta);
        if let Err(error) = self.transport.update(delta, &mut self.client) {
            bail!("{error:?}");
        }
        if self.client.is_disconnected() {
            bail!("{:?}", self.client.disconnect_reason());
        }
        for channel in CHANNELS {
            while let Some(bytes) = self.client.receive_message(channel) {
                let Some(message) = decoded::<ServerMessage>(&bytes) else {
                    continue;
                };
                let unreliable = message.lane() == Lane::Unreliable;
                if unreliable && self.impairment.drops() {
                    continue;
                }
                if self.impairment.delays() {
                    self.inbound.push(now, self.impairment.delay(unreliable), message);
                } else if sink(message).is_break() {
                    return Ok(());
                }
            }
        }
        while let Some(message) = self.inbound.pop_due(now) {
            if sink(message).is_break() {
                break;
            }
        }
        Ok(())
    }

    // Hands the frame's sends to renet and puts their packets on the wire.
    // Nothing is handed over before the handshake completes, so `CLogin`
    // waits here rather than in a channel that is not open yet.
    pub fn flush(&mut self, now: Instant) {
        if self.client.is_connected() {
            while let Ok(message) = self.outgoing.try_recv() {
                let unreliable = message.lane() == Lane::Unreliable;
                if unreliable && self.impairment.drops() {
                    continue;
                }
                if self.impairment.delays() {
                    self.outbound.push(now, self.impairment.delay(unreliable), message);
                } else {
                    send(&mut self.client, &message);
                }
            }
            while let Some(message) = self.outbound.pop_due(now) {
                send(&mut self.client, &message);
            }
        }
        if let Err(error) = self.transport.send_packets(&mut self.client) {
            debug!("packets not sent: {error:?}");
        }
    }

    // Tells the server at once instead of leaving it to time out.
    pub fn disconnect(&mut self) {
        self.transport.disconnect();
    }
}

// An undecodable message is a bug on the sending side and is skipped, not fatal.
fn decoded<T: Decode<()>>(bytes: &[u8]) -> Option<T> {
    match decode_message(bytes) {
        Ok(message) => Some(message),
        Err(error) => {
            warn!("skipping an undecodable server message: {error}");
            None
        }
    }
}

fn send(client: &mut RenetClient, message: &ClientMessage) {
    match encode_message(message) {
        Ok(bytes) => client.send_message(channel_for(message.lane(), bytes.len()), bytes),
        Err(error) => error!("dropping a message to the server: {error}"),
    }
}

#[cfg(test)]
#[path = "tests/transport.rs"]
mod tests;
