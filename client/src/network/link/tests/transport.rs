use std::{
    net::UdpSocket,
    ops::ControlFlow,
    thread,
    time::{Duration, Instant},
};

use renet::{RenetServer, ServerEvent};
use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};

use super::*;
use common::{
    network::{PROTOCOL_ID, UNRELIABLE_CHANNEL},
    protocol::{CPing, ClientMessage},
};

// The client's netcode clock is real time and it sends once per 250 ms while
// connecting, so the handshake takes a few real steps.
const STEP: Duration = Duration::from_millis(50);
const DEADLINE: Duration = Duration::from_secs(5);

// A server driven straight through renet, since the server crate is not
// available here.
struct RawServer {
    server: RenetServer,
    transport: NetcodeServerTransport,
}

impl RawServer {
    fn listen() -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("loopback socket unavailable");
        let local_addr = socket.local_addr().expect("socket has no address");
        let config = ServerConfig {
            current_time: unix_now(),
            max_clients: 1,
            protocol_id: PROTOCOL_ID,
            public_addresses: vec![local_addr],
            authentication: ServerAuthentication::Unsecure,
        };
        let transport = NetcodeServerTransport::new(config, socket).expect("server transport failed to start");
        Self {
            server: RenetServer::new(connection_config()),
            transport,
        }
    }

    fn addr(&self) -> SocketAddr {
        self.transport.addresses()[0]
    }

    fn step(&mut self) -> Vec<ServerEvent> {
        self.server.update(STEP);
        let _ = self.transport.update(STEP, &mut self.server);
        let events = std::iter::from_fn(|| self.server.get_event()).collect();
        self.transport.send_packets(&mut self.server);
        events
    }
}

fn pump(link: &mut ServerLink) {
    let now = Instant::now();
    link.receive(now, |_| ControlFlow::Continue(())).expect("link closed");
    link.flush(now);
}

fn connected(link: &ServerLink) -> bool {
    match link {
        ServerLink::Remote(link) => link.client.is_connected(),
        ServerLink::Local(_) => false,
    }
}

#[test]
fn remote_link_connects_sends_and_disconnects_at_once() {
    let mut server = RawServer::listen();
    let (to_server, mut link) = connect(server.addr(), Impairment::default()).expect("connect failed");

    let deadline = Instant::now() + DEADLINE;
    while !connected(&link) {
        assert!(Instant::now() < deadline, "handshake did not complete");
        server.step();
        pump(&mut link);
        thread::sleep(STEP);
    }
    let client_id = *server.server.clients_id().first().expect("server has no client");

    to_server
        .send(ClientMessage::Ping(CPing { timestamp_nanos: 7 }))
        .expect("send failed");
    pump(&mut link);
    let mut ping = None;
    while ping.is_none() {
        assert!(Instant::now() < deadline, "ping did not arrive");
        server.step();
        ping = server
            .server
            .receive_message(client_id, UNRELIABLE_CHANNEL)
            .map(|bytes| decode_message::<ClientMessage>(&bytes).expect("undecodable ping"));
        thread::sleep(STEP);
    }
    assert!(matches!(ping, Some(ClientMessage::Ping(CPing { timestamp_nanos: 7 }))));

    link.disconnect();
    loop {
        assert!(Instant::now() < deadline, "disconnect did not reach the server");
        let events = server.step();
        if events
            .iter()
            .any(|event| matches!(event, ServerEvent::ClientDisconnected { .. }))
        {
            break;
        }
        thread::sleep(STEP);
    }
}
