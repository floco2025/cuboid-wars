use super::fixtures::{connect, server_app, server_app_with_options};
use super::*;
use crate::players::PlayerCheckpoint;
use common::{
    config::GameplayConfig,
    constants::{TICK_DURATION, TICK_SECS},
    protocol::{
        CAdmin, CLogin, CMove, ClientMessage, Health, ItemType, MapLayout, PlayerGeneration, PlayerId,
        PlayerMoveIntent, PlayerMovementState, Position, ServerMessage,
    },
};
use crossbeam_channel::Receiver;

fn feed_texts(receiver: &Receiver<ServerMessage>) -> Vec<String> {
    std::iter::from_fn(|| receiver.try_recv().ok())
        .filter_map(|message| match message {
            ServerMessage::Feed(feed) => Some(feed.spans.into_iter().map(|span| span.text).collect()),
            _ => None,
        })
        .collect()
}

fn saved_checkpoint(app: &App, id: PlayerId) -> u32 {
    app.world()
        .resource::<PlayerMap>()
        .get(&id)
        .expect("logged-in player missing")
        .session
        .checkpoint
        .number
}

fn in_checkpoint(app: &App, number: u32, pos: &Position) -> bool {
    app.world()
        .resource::<MapLayout>()
        .checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.number == number)
        .any(|checkpoint| {
            (checkpoint.min_x..=checkpoint.max_x).contains(&pos.x)
                && (checkpoint.min_z..=checkpoint.max_z).contains(&pos.z)
        })
}

#[test]
fn checkpoint_option_starts_every_login_at_the_numbered_checkpoint_and_saves_it() {
    let options = |checkpoint: u32| ServerAppOptions {
        map: None,
        god: false,
        peace: false,
        initial_spawn: None,
        checkpoint: Some(checkpoint),
        network: NetworkOverrides::default(),
        logging: false,
    };
    let error = server_app_with_options(options(7), None)
        .expect_err("unknown checkpoint accepted")
        .to_string();
    assert!(
        error.contains("unknown checkpoint 7") && error.contains("checkpoints are 0, 1"),
        "{error}"
    );

    let mut app = server_app_with_options(options(1), None).expect("server app failed to initialize");
    let (client, receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin { name: "Player".into() }))
        .expect("login failed");
    app.update();
    let relocation = std::iter::from_fn(|| receiver.try_recv().ok())
        .find_map(|message| match message {
            ServerMessage::PlayerRelocated(relocation) => Some(relocation),
            _ => None,
        })
        .expect("login relocation missing");
    let pos = relocation.player.movement.pos;
    assert!(in_checkpoint(&app, 1, &pos), "{pos:?} is outside checkpoint 1");
    assert_eq!(saved_checkpoint(&app, PlayerId(1)), 1);
}

#[test]
fn checkpoint_command_reports_and_sets_the_senders_checkpoint() {
    let mut app = server_app(NetworkOverrides::default()).expect("server app failed to initialize");
    let (client, receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin { name: "Player".into() }))
        .expect("login failed");
    app.update();
    while receiver.try_recv().is_ok() {}
    let reply = |app: &mut App, command: &str| {
        client
            .send(ClientMessage::Admin(CAdmin {
                command: command.into(),
            }))
            .expect("admin command delivery failed");
        app.update();
        feed_texts(&receiver)
    };
    assert_eq!(reply(&mut app, "/checkpoint"), vec!["checkpoint: 0"]);
    assert_eq!(
        reply(&mut app, "/checkpoint nowhere"),
        vec!["usage: /checkpoint [number]"]
    );
    assert_eq!(
        reply(&mut app, "/checkpoint 7"),
        vec!["unknown checkpoint 7: the map's checkpoints are 0, 1"]
    );
    assert_eq!(saved_checkpoint(&app, PlayerId(1)), 0);
    assert_eq!(reply(&mut app, "/checkpoint 1"), vec!["checkpoint set to 1"]);
    assert_eq!(saved_checkpoint(&app, PlayerId(1)), 1);
    assert_eq!(reply(&mut app, "/checkpoint"), vec!["checkpoint: 1"]);
}

#[test]
fn return_command_relocates_the_living_sender_to_its_checkpoint_without_a_death() {
    let mut app = server_app(NetworkOverrides::default()).expect("server app failed to initialize");
    let (client, receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin { name: "Player".into() }))
        .expect("login failed");
    app.update();
    while receiver.try_recv().is_ok() {}
    let id = PlayerId(1);
    let body = |app: &App| {
        app.world()
            .resource::<PlayerMap>()
            .get(&id)
            .expect("logged-in player missing")
            .entity()
            .expect("player has no body")
    };
    let entity = body(&app);
    let generation = |app: &App| {
        app.world()
            .resource::<PlayerMap>()
            .get(&id)
            .expect("logged-in player missing")
            .session
            .generation
    };
    let before = generation(&app);
    app.world_mut().entity_mut(entity).insert((
        Position {
            x: 40.0,
            y: 3.0,
            z: 40.0,
        },
        Health(17.0),
    ));
    {
        let mut players = app.world_mut().resource_mut::<PlayerMap>();
        let player = players.get_mut(&id).expect("logged-in player missing");
        player.session.score = 9;
        player.session.checkpoint = PlayerCheckpoint::numbered(1);
    }
    client
        .send(ClientMessage::Admin(CAdmin {
            command: "/return".into(),
        }))
        .expect("admin command delivery failed");
    app.update();
    let messages: Vec<_> = std::iter::from_fn(|| receiver.try_recv().ok()).collect();
    let relocation = messages
        .iter()
        .find_map(|message| match message {
            ServerMessage::PlayerRelocated(relocation) => Some(relocation),
            _ => None,
        })
        .expect("return sent no relocation");
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, ServerMessage::PlayerDeath(_))),
        "a return is no death"
    );
    assert_eq!(body(&app), entity, "the body stays; only its generation advances");
    assert_ne!(generation(&app), before);
    assert_eq!(relocation.player.generation, generation(&app));
    assert!(in_checkpoint(&app, 1, &relocation.player.movement.pos));
    assert_eq!(relocation.player.health.0, 17.0);
    let players = app.world().resource::<PlayerMap>();
    let player = players.get(&id).expect("logged-in player missing");
    assert_eq!(player.session.score, 9);
    assert_eq!(player.session.checkpoint.number, 1);
    assert!(messages.iter().any(|message| matches!(
        message,
        ServerMessage::Feed(feed) if feed.spans.iter().any(|span| span.text.contains("returned to checkpoint 1"))
    )));
}

#[test]
fn simultaneous_returns_reserve_their_destinations() {
    let mut app = server_app(NetworkOverrides::default()).expect("server app failed to initialize");
    let (first, receiver) = connect(&mut app);
    let (second, _second_receiver) = connect(&mut app);
    for client in [&first, &second] {
        client
            .send(ClientMessage::Login(CLogin { name: "Player".into() }))
            .expect("login failed");
    }
    app.update();
    while receiver.try_recv().is_ok() {}

    // A body as wide as the start's only cell makes its centre the only spawn candidate.
    let cell_size = app.world().resource::<crate::map::MapConfig>().grids[0]
        .geometry
        .cell_size();
    {
        let mut gameplay = app.world_mut().resource_mut::<GameplayConfig>();
        gameplay.player.movement_collider.diameter = cell_size;
        gameplay.player.movement_collider.height = cell_size;
    }
    {
        let mut layout = app.world_mut().resource_mut::<MapLayout>();
        let start = &mut layout.checkpoints[0];
        start.rows = [0, 1];
        start.max_z = start.min_z + cell_size;
    }
    for (id, x) in [(PlayerId(1), 40.0), (PlayerId(2), 50.0)] {
        let pos = Position { x, y: 0.0, z: 40.0 };
        let entity = {
            let mut players = app.world_mut().resource_mut::<PlayerMap>();
            let info = players.get_mut(&id).expect("player missing");
            info.life.movement = PlayerMovementState::new(pos, PlayerMoveIntent::Idle, 0.0, 0.0);
            info.entity().expect("body missing")
        };
        app.world_mut().entity_mut(entity).insert(pos);
    }
    for client in [&first, &second] {
        client
            .send(ClientMessage::Admin(CAdmin {
                command: "/return".into(),
            }))
            .expect("return delivery failed");
    }
    app.update();

    let positions: Vec<_> = [PlayerId(1), PlayerId(2)]
        .iter()
        .map(|id| {
            let entity = app
                .world()
                .resource::<PlayerMap>()
                .get(id)
                .expect("player missing")
                .entity()
                .expect("body missing");
            *app.world().get::<Position>(entity).expect("position missing")
        })
        .collect();
    assert_eq!(positions.iter().filter(|pos| in_checkpoint(&app, 0, pos)).count(), 1);
    assert_ne!(positions[0], positions[1], "both returns claimed the only spot");
    let relocations = std::iter::from_fn(|| receiver.try_recv().ok())
        .filter(|message| matches!(message, ServerMessage::PlayerRelocated(_)))
        .count();
    assert_eq!(relocations, 1, "the blocked return must not relocate its player");
}

#[test]
fn returns_preserve_heals_in_the_same_ingress_batch() {
    for commands in [
        vec!["/heal", "/return"],
        vec!["/return", "/heal"],
        vec!["/return", "/heal", "/return"],
    ] {
        let mut app = server_app(NetworkOverrides::default()).expect("server app failed to initialize");
        let (client, receiver) = connect(&mut app);
        client
            .send(ClientMessage::Login(CLogin { name: "Player".into() }))
            .expect("login failed");
        app.update();
        while receiver.try_recv().is_ok() {}
        let entity = app
            .world()
            .resource::<PlayerMap>()
            .get(&PlayerId(1))
            .expect("player missing")
            .entity()
            .expect("body missing");
        let initial_health = 17.0;
        app.world_mut().entity_mut(entity).insert(Health(initial_health));
        let max_health = app.world().resource::<ServerGameplayConfig>().combat.health.player.max;
        let mut expected_health = initial_health;
        let mut expected_relocations = Vec::new();
        for command in &commands {
            if *command == "/heal" {
                expected_health = max_health;
            } else {
                expected_relocations.push(expected_health);
            }
            client
                .send(ClientMessage::Admin(CAdmin {
                    command: (*command).into(),
                }))
                .expect("admin command delivery failed");
        }
        app.update();
        assert_eq!(app.world().get::<Health>(entity).expect("health missing").0, max_health);
        let relocation_health: Vec<_> = std::iter::from_fn(|| receiver.try_recv().ok())
            .filter_map(|message| match message {
                ServerMessage::PlayerRelocated(message) => Some(message.player.health.0),
                _ => None,
            })
            .collect();
        assert_eq!(relocation_health, expected_relocations, "{commands:?}");
    }
}

#[test]
fn startup_god_and_peace_share_the_console_state() {
    for god in [false, true] {
        for peace in [false, true] {
            let mut app = server_app_with_options(
                ServerAppOptions {
                    map: None,
                    god,
                    peace,
                    initial_spawn: None,
                    checkpoint: None,
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
fn initial_spawn_override_places_the_first_single_player_body_exactly() {
    let expected = Position {
        x: -8.5,
        y: 3.25,
        z: 11.0,
    };
    let mut app = server_app_with_options(
        ServerAppOptions {
            map: None,
            god: false,
            peace: false,
            initial_spawn: Some(expected),
            checkpoint: None,
            network: NetworkOverrides::default(),
            logging: false,
        },
        None,
    )
    .expect("server app failed to initialize");
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
    let player = snapshot.players.first().expect("spawned player missing");
    assert_eq!(player.1.movement.pos, expected);
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
