use std::{
    net::{SocketAddr, UdpSocket},
    time::Duration,
};

use bevy::prelude::App;
use renet::{Bytes, RenetClient};
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};

use super::listen;
use crate::{
    app::{
        NetworkOverrides,
        fixtures::{connect, server_app, server_app_with_listener},
    },
    players::PlayerMap,
};
use common::{
    network::{CHANNELS, PROTOCOL_ID, RELIABLE_CHANNEL, connection_config, decode_message, encode_message, unix_now},
    protocol::{CLogin, ClientMessage, PlayerId, ServerMessage},
};

// netcode lets a client send once per 250 ms of its own clock, so each
// synthetic step is that long and the handshake needs no real waiting.
const STEP: Duration = Duration::from_millis(250);
const ROUNDS: usize = 200;

// A client driven straight through renet, since the client crate is not
// available here.
struct RawClient {
    client: RenetClient,
    transport: NetcodeClientTransport,
}

impl RawClient {
    fn connect(server: SocketAddr) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("loopback socket unavailable");
        let authentication = ClientAuthentication::Unsecure {
            protocol_id: PROTOCOL_ID,
            client_id: 7,
            server_addr: server,
            user_data: None,
        };
        let transport =
            NetcodeClientTransport::new(unix_now(), authentication, socket).expect("client transport failed to start");
        Self {
            client: RenetClient::new(connection_config()),
            transport,
        }
    }

    // One exchange: the client reads the server's packets and sends its own,
    // then the server runs a tick. Returns what the client received.
    fn exchange(&mut self, app: &mut App) -> Vec<ServerMessage> {
        self.client.update(STEP);
        let _ = self.transport.update(STEP, &mut self.client);
        let mut received = Vec::new();
        for channel in CHANNELS {
            while let Some(bytes) = self.client.receive_message(channel) {
                received.push(decode_message::<ServerMessage>(&bytes).expect("server sent an undecodable message"));
            }
        }
        let _ = self.transport.send_packets(&mut self.client);
        app.update();
        received
    }

    fn send(&mut self, message: &ClientMessage) {
        let bytes = encode_message(message).expect("message failed to encode");
        self.client.send_message(RELIABLE_CHANNEL, bytes);
    }
}

fn listening_app() -> (App, RawClient) {
    let listener = listen("127.0.0.1:0".parse().expect("loopback address invalid")).expect("listen failed");
    let server = listener.local_addr();
    let app = server_app_with_listener(NetworkOverrides::default(), Some(listener))
        .expect("listening server app did not build");
    let raw = RawClient::connect(server);
    (app, raw)
}

fn logged_in(app: &App, id: PlayerId) -> bool {
    app.world()
        .resource::<PlayerMap>()
        .get(&id)
        .is_some_and(|player| player.connection.logged_in)
}

fn login_over_loopback(app: &mut App, raw: &mut RawClient) -> PlayerId {
    for _ in 0..ROUNDS {
        raw.exchange(app);
        if raw.client.is_connected() {
            break;
        }
    }
    assert!(raw.client.is_connected(), "netcode handshake did not complete");
    raw.send(&ClientMessage::Login(CLogin { name: "Remote".into() }));
    let id = PlayerId(1);
    for _ in 0..ROUNDS {
        let received = raw.exchange(app);
        if received.iter().any(|message| matches!(message, ServerMessage::Init(_))) {
            assert!(logged_in(app, id));
            return id;
        }
    }
    panic!("SInit did not arrive over loopback");
}

#[test]
fn remote_client_logs_in_over_loopback_and_leaves_on_hang_up() {
    let (mut app, mut raw) = listening_app();
    let id = login_over_loopback(&mut app, &mut raw);

    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&id)
        .expect("remote player missing")
        .connection
        .hang_up();
    for _ in 0..ROUNDS {
        raw.exchange(&mut app);
        if raw.client.is_disconnected() && app.world().resource::<PlayerMap>().get(&id).is_none() {
            return;
        }
    }
    panic!("hang-up did not disconnect the remote client");
}

#[test]
fn undecodable_bytes_from_a_remote_client_are_skipped() {
    let (mut app, mut raw) = listening_app();
    let id = login_over_loopback(&mut app, &mut raw);

    raw.client
        .send_message(RELIABLE_CHANNEL, Bytes::from_static(b"not a message"));
    for _ in 0..3 {
        raw.exchange(&mut app);
    }
    assert!(raw.client.is_connected());
    assert!(logged_in(&app, id));
}

#[test]
fn a_server_without_a_listener_still_serves_local_links() {
    let mut app = server_app(NetworkOverrides::default()).expect("server app failed");
    let (client, receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin { name: "Local".into() }))
        .expect("login failed");
    app.update();
    assert!(logged_in(&app, PlayerId(1)));
    assert!(std::iter::from_fn(|| receiver.try_recv().ok()).any(|message| matches!(message, ServerMessage::Init(_))));
}
