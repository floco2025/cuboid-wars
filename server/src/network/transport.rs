use std::{
    net::{SocketAddr, UdpSocket},
    time::Instant,
};

use anyhow::{Context, Result};
use bevy::prelude::*;
use renet::RenetServer;
use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};

use common::network::{MAX_CLIENTS, PROTOCOL_ID, connection_config, unix_now};

// The UDP endpoint remote clients join through; absent in single-player.
#[derive(Resource)]
pub struct Listener {
    pub server: RenetServer,
    transport: NetcodeServerTransport,
    local_addr: SocketAddr,
    // renet and netcode time out on their own clocks, so they advance by wall
    // time rather than tick time, which skips overruns.
    polled: Instant,
}

impl Listener {
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    // Advances the connections by the wall time since the last poll and reads
    // every waiting packet.
    pub fn poll(&mut self) -> Result<()> {
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.polled);
        self.polled = now;
        self.server.update(delta);
        self.transport
            .update(delta, &mut self.server)
            .map_err(|error| anyhow::anyhow!("{error:?}"))
    }

    pub fn send_packets(&mut self) {
        self.transport.send_packets(&mut self.server);
    }
}

pub fn listen(bind: SocketAddr) -> Result<Listener> {
    let socket = UdpSocket::bind(bind).with_context(|| format!("failed to bind {bind}"))?;
    let local_addr = socket.local_addr()?;
    let config = ServerConfig {
        current_time: unix_now(),
        max_clients: MAX_CLIENTS,
        protocol_id: PROTOCOL_ID,
        public_addresses: vec![local_addr],
        authentication: ServerAuthentication::Unsecure,
    };
    let transport = NetcodeServerTransport::new(config, socket)?;
    println!("listening on {local_addr}");
    Ok(Listener {
        server: RenetServer::new(connection_config()),
        transport,
        local_addr,
        polled: Instant::now(),
    })
}

#[cfg(test)]
#[path = "tests/transport.rs"]
mod tests;
