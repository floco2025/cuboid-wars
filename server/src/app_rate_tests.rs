use bevy::prelude::*;
use common::{config::NetworkConfig, protocol::*};
use tokio::sync::mpsc::unbounded_channel;

use super::{NetworkOverrides, build_server_app};
use crate::network::{ClientToServer, FromClientsChannel, ServerToClient};

#[test]
fn sixty_hz_reaches_bootstrap_advances_one_second_and_preserves_network_cadences() {
    let (incoming, receiver) = unbounded_channel();
    let mut app = build_server_app(
        Some("obby"),
        NetworkOverrides {
            server_hz: Some(60),
            update_hz: Some(30),
            snapshot_hz: Some(4),
        },
        FromClientsChannel::new(receiver),
    )
    .expect("60 Hz server config rejected");
    let (sender, mut receiver) = unbounded_channel();
    incoming
        .send((PlayerId(1), ClientToServer::Registration { to_client: sender }))
        .expect("registration failed");
    incoming
        .send((
            PlayerId(1),
            ClientToServer::Message(ClientMessage::Login(CLogin { name: "Player".into() })),
        ))
        .expect("login failed");
    // Movement batches carry the other players, so the counted client needs company.
    let (observed, _observed_receiver) = unbounded_channel();
    incoming
        .send((PlayerId(2), ClientToServer::Registration { to_client: observed }))
        .expect("registration failed");
    incoming
        .send((
            PlayerId(2),
            ClientToServer::Message(ClientMessage::Login(CLogin { name: "Other".into() })),
        ))
        .expect("login failed");
    app.update();
    let init = std::iter::from_fn(|| receiver.try_recv().ok())
        .find_map(|message| match message {
            ServerToClient::Send(ServerMessage::Init(init)) => Some(init),
            _ => None,
        })
        .expect("bootstrap missing");
    assert_eq!(init.world.network.server_hz, 60);
    assert_eq!(init.world.network.update_hz, 30);
    assert_eq!(init.world.network.snapshot_hz, 4);
    while receiver.try_recv().is_ok() {}
    let start_tick = app.world().resource::<ServerTick>().0;
    let start_time = app.world().resource::<Time>().elapsed_secs();
    for _ in 0..60 {
        app.update();
    }
    assert_eq!(app.world().resource::<ServerTick>().0 - start_tick, 60);
    assert!((app.world().resource::<Time>().elapsed_secs() - start_time - 1.0).abs() < 1e-5);
    let mut moves = 0;
    let mut snapshots = 0;
    for message in std::iter::from_fn(|| receiver.try_recv().ok()) {
        match message {
            ServerToClient::Send(ServerMessage::PlayerMoves(_)) => moves += 1,
            ServerToClient::Send(ServerMessage::Snapshot(_)) => snapshots += 1,
            _ => {}
        }
    }
    assert_eq!(moves, 30);
    assert_eq!(snapshots, 4);
    assert_eq!(app.world().resource::<NetworkConfig>().server_hz, 60);
}

#[test]
fn cli_rates_are_checked_together_after_overrides() {
    for (server, updates, snapshots) in [(0, 1, 1), (30, 60, 4), (60, 30, 61)] {
        let (_, receiver) = unbounded_channel();
        assert!(
            build_server_app(
                Some("obby"),
                NetworkOverrides {
                    server_hz: Some(server),
                    update_hz: Some(updates),
                    snapshot_hz: Some(snapshots),
                },
                FromClientsChannel::new(receiver)
            )
            .is_err()
        );
    }
}
