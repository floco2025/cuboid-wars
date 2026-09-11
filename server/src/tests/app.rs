use super::fixtures::{connect, server_app, server_app_with_options};
use super::*;
use common::{
    config::GameplayConfig,
    constants::{TICK_DURATION, TICK_SECS},
    protocol::{
        CAdmin, CLogin, CMove, ClientMessage, ItemType, PlayerGeneration, PlayerId, PlayerMoveIntent,
        PlayerMovementState, Position, ServerMessage,
    },
};

#[test]
fn startup_god_and_peace_share_the_console_state() {
    for god in [false, true] {
        for peace in [false, true] {
            let mut app = server_app_with_options(
                ServerAppOptions {
                    map: None,
                    god,
                    peace,
                    network: NetworkOverrides::default(),
                    logging: false,
                },
                None,
            )
            .expect("server app failed to initialize");
            assert_eq!(app.world().resource::<Invincibility>().0, god);
            assert_eq!(app.world().resource::<ActorMap>().peaceful, peace);
            let (client, receiver) = connect(&mut app);
            client
                .send(ClientMessage::Login(CLogin { name: "Player".into() }))
                .expect("login failed");
            app.update();
            let snapshot = std::iter::from_fn(|| receiver.try_recv().ok())
                .find_map(|message| match message {
                    ServerMessage::Snapshot(snapshot) => Some(snapshot),
                    _ => None,
                })
                .expect("initial snapshot missing");
            assert_eq!(snapshot.actors_peaceful, peace);

            for (command, expected_god, expected_peace) in [("/god", !god, peace), ("/peace", !god, !peace)] {
                client
                    .send(ClientMessage::Admin(CAdmin {
                        command: command.into(),
                    }))
                    .expect("admin command delivery failed");
                app.update();
                assert_eq!(app.world().resource::<Invincibility>().0, expected_god);
                assert_eq!(app.world().resource::<ActorMap>().peaceful, expected_peace);
            }
        }
    }
}

#[test]
fn give_missiles_sends_weapon_selection_cue_even_when_ammo_is_full() {
    let mut app = server_app(NetworkOverrides {
        snapshot_hz: Some(1),
        ..default()
    })
    .expect("server app failed to initialize");
    let id = PlayerId(1);
    let (client, receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin { name: "Player".into() }))
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
        client
            .send(ClientMessage::Admin(CAdmin {
                command: "/give missiles".into(),
            }))
            .expect("admin command delivery failed");
        app.update();
        let statuses: Vec<_> = std::iter::from_fn(|| receiver.try_recv().ok())
            .filter_map(|message| match message {
                ServerMessage::PlayerStatus(status) => Some(status),
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
    let mut app = server_app(NetworkOverrides {
        update_hz: Some(30),
        ..default()
    })
    .expect("test server app did not build");
    app.update();
    let mut clients = Vec::new();
    for id in [PlayerId(1), PlayerId(2)] {
        let (client, receiver) = connect(&mut app);
        client
            .send(ClientMessage::Login(CLogin {
                name: format!("Player {}", id.0),
            }))
            .expect("login failed");
        clients.push((id, client, receiver));
    }
    app.update();
    let targets: Vec<_> = clients
        .iter()
        .map(|(id, _, _)| {
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
    for ((_, client, _), (_, _, pos)) in clients.iter().zip(&targets) {
        client
            .send(ClientMessage::Move(CMove {
                generation: PlayerGeneration(0),
                seq: 1,
                portal_crossing: 0,
                movement: PlayerMovementState::new(*pos, PlayerMoveIntent::Idle, 0.0, 0.0),
            }))
            .expect("movement delivery failed");
    }
    app.update();
    for (_, entity, pos) in &targets {
        assert_eq!(
            app.world().get::<Position>(*entity).expect("player position missing"),
            pos
        );
    }
    for (id, _, receiver) in &mut clients {
        let latest = std::iter::from_fn(|| receiver.try_recv().ok())
            .filter_map(|message| match message {
                ServerMessage::PlayerMoves(moves) => Some(moves),
                _ => None,
            })
            .last()
            .expect("movement broadcast missing");
        assert_eq!(latest.moves.len(), 1);
        assert!(latest.moves.iter().all(|entry| entry.id != *id && entry.seq == 1));
    }
    app.update();
    for (id, _, receiver) in &mut clients {
        let latest = std::iter::from_fn(|| receiver.try_recv().ok())
            .filter_map(|message| match message {
                ServerMessage::PlayerMoves(moves) => Some(moves),
                _ => None,
            })
            .last()
            .expect("live movement missing");
        assert!(latest.moves.iter().all(|entry| entry.id != *id && entry.seq == 1));
    }
}

#[test]
fn independent_rate_overrides_reach_init_and_leave_simulation_unchanged() {
    let mut app = server_app(NetworkOverrides {
        update_hz: Some(2),
        snapshot_hz: Some(7),
        ..default()
    })
    .expect("server app failed");
    let (client, receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin { name: "Player".into() }))
        .expect("login failed");
    // Movement batches carry the other players, so the counted client needs company.
    let (other, _other_receiver) = connect(&mut app);
    other
        .send(ClientMessage::Login(CLogin { name: "Other".into() }))
        .expect("login failed");
    app.update();
    let init = std::iter::from_fn(|| receiver.try_recv().ok())
        .filter_map(|message| match message {
            ServerMessage::Init(init) => Some(init),
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
            ServerMessage::PlayerMoves(_) => moves += 1,
            ServerMessage::Snapshot(_) => snapshots += 1,
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
