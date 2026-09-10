use super::*;
use crate::network::{ClientToServer, ServerToClient};
use common::{
    config::GameplayConfig,
    constants::{TICK_DURATION, TICK_SECS},
    protocol::{
        CAdmin, CLogin, CMove, ClientMessage, ItemType, PlayerGeneration, PlayerId, PlayerMoveIntent,
        PlayerMovementState, Position, ServerMessage,
    },
};
use tokio::sync::mpsc::unbounded_channel;

#[test]
fn give_missiles_sends_weapon_selection_cue_even_when_ammo_is_full() {
    let (incoming, receiver) = unbounded_channel();
    let mut app = build_server_app(
        Some("obby"),
        NetworkOverrides {
            snapshot_hz: Some(1),
            ..default()
        },
        FromClientsChannel::new(receiver),
    )
    .expect("server app failed to initialize");
    let id = PlayerId(1);
    let (sender, mut receiver) = unbounded_channel();
    incoming
        .send((id, ClientToServer::Registration { to_client: sender }))
        .expect("registration failed");
    incoming
        .send((
            id,
            ClientToServer::Message(ClientMessage::Login(CLogin { name: "Player".into() })),
        ))
        .expect("login failed");
    app.update();
    while receiver.try_recv().is_ok() {}

    let max = app.world().resource::<GameplayConfig>().missiles.max_missiles;
    for ammo in [0, max / 2, max] {
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .get_mut(&id)
            .expect("logged-in player missing")
            .life
            .missiles = ammo;
        incoming
            .send((
                id,
                ClientToServer::Message(ClientMessage::Admin(CAdmin {
                    command: "/give missiles".into(),
                })),
            ))
            .expect("admin command delivery failed");
        app.update();
        let statuses: Vec<_> = std::iter::from_fn(|| receiver.try_recv().ok())
            .filter_map(|message| match message {
                ServerToClient::Send(ServerMessage::PlayerStatus(status)) => Some(status),
                _ => None,
            })
            .collect();
        assert_eq!(
            statuses.len(),
            1,
            "grant must request weapon selection once at {ammo} ammo"
        );
        let status = &statuses[0];
        let players = app.world().resource::<PlayerMap>();
        let player = players.get(&id).expect("logged-in player missing");
        assert_eq!(status.id, id);
        assert_eq!(status.generation, player.session.generation);
        assert_eq!(status.collected, Some(ItemType::MissilePack));
        assert_eq!(status.missiles, max);
        assert_eq!(player.life.missiles, max);
    }
}

#[test]
fn full_server_schedule_broadcasts_the_latest_sample_to_both_clients() {
    let (incoming, receiver) = unbounded_channel();
    let mut app = build_server_app(
        Some("obby"),
        NetworkOverrides {
            update_hz: Some(30),
            ..default()
        },
        FromClientsChannel::new(receiver),
    )
    .expect("obby server app did not build");
    app.update();
    let mut receivers = Vec::new();
    for id in [PlayerId(1), PlayerId(2)] {
        let (sender, receiver) = unbounded_channel();
        incoming
            .send((id, ClientToServer::Registration { to_client: sender }))
            .expect("registration failed");
        incoming
            .send((
                id,
                ClientToServer::Message(ClientMessage::Login(CLogin {
                    name: format!("Player {}", id.0),
                })),
            ))
            .expect("login failed");
        receivers.push((id, receiver));
    }
    app.update();
    let targets: Vec<_> = receivers
        .iter()
        .map(|(id, _)| {
            let entity = app
                .world()
                .resource::<PlayerMap>()
                .get(id)
                .and_then(|info| info.entity())
                .expect("player body missing");
            let mut pos = *app.world().get::<Position>(entity).expect("player position missing");
            pos.x += 0.2;
            (*id, entity, pos)
        })
        .collect();
    for (id, _, pos) in &targets {
        incoming
            .send((
                *id,
                ClientToServer::Message(ClientMessage::Move(CMove {
                    generation: PlayerGeneration(0),
                    seq: 1,
                    portal_crossing: 0,
                    movement: PlayerMovementState::new(*pos, PlayerMoveIntent::Idle, 0.0, 0.0),
                })),
            ))
            .expect("movement delivery failed");
    }
    app.update();
    for (_, entity, pos) in &targets {
        assert_eq!(
            app.world().get::<Position>(*entity).expect("player position missing"),
            pos
        );
    }
    for (id, receiver) in &mut receivers {
        let latest = std::iter::from_fn(|| receiver.try_recv().ok())
            .filter_map(|message| match message {
                ServerToClient::Send(ServerMessage::PlayerMoves(moves)) => Some(moves),
                _ => None,
            })
            .last()
            .expect("movement broadcast missing");
        assert_eq!(latest.moves.len(), 1);
        assert!(latest.moves.iter().all(|entry| entry.id != *id && entry.seq == 1));
    }
    app.update();
    for (id, receiver) in &mut receivers {
        let latest = std::iter::from_fn(|| receiver.try_recv().ok())
            .filter_map(|message| match message {
                ServerToClient::Send(ServerMessage::PlayerMoves(moves)) => Some(moves),
                _ => None,
            })
            .last()
            .expect("live movement missing");
        assert!(latest.moves.iter().all(|entry| entry.id != *id && entry.seq == 1));
    }
}

#[test]
fn independent_rate_overrides_reach_init_and_leave_simulation_unchanged() {
    let (incoming, receiver) = unbounded_channel();
    let mut app = build_server_app(
        Some("obby"),
        NetworkOverrides {
            update_hz: Some(2),
            snapshot_hz: Some(7),
            ..default()
        },
        FromClientsChannel::new(receiver),
    )
    .expect("server app failed");
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
        .filter_map(|message| match message {
            ServerToClient::Send(ServerMessage::Init(init)) => Some(init),
            _ => None,
        })
        .last()
        .expect("init missing");
    assert_eq!(init.world.network.update_hz, 2);
    assert_eq!(init.world.network.snapshot_hz, 7);
    assert_eq!(app.world().resource::<NetworkConfig>().update_hz, 2);
    let before = app.world().resource::<ServerTick>().0;
    for _ in 0..60 {
        app.update();
    }
    assert_eq!(app.world().resource::<ServerTick>().0.wrapping_sub(before), 60);
    let mut moves = 0;
    let mut snapshots = 0;
    for message in std::iter::from_fn(|| receiver.try_recv().ok()) {
        match message {
            ServerToClient::Send(ServerMessage::PlayerMoves(_)) => moves += 1,
            ServerToClient::Send(ServerMessage::Snapshot(_)) => snapshots += 1,
            _ => {}
        }
    }
    assert_eq!(moves, 4);
    assert_eq!(snapshots, 14);
}

#[test]
fn server_time_advances_exactly_one_tick_per_update() {
    let mut app = App::new();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(TICK_DURATION));
    app.add_plugins(MinimalPlugins);
    // The first update only records the start instant.
    for _ in 0..3 {
        app.update();
    }

    let time = app.world().resource::<Time>();
    assert!((time.delta_secs() - TICK_SECS).abs() < 1e-6);
    assert!((time.elapsed_secs() - 2.0 * TICK_SECS).abs() < 1e-6);
}
