use crossbeam_channel::{Receiver, Sender};

use super::{
    fixtures::{connect, options, server_app, server_app_with_options},
    *,
};
use crate::{map::MapConfig, players::PlayerCheckpoint};
use common::{
    config::GameplayConfig,
    protocol::{
        CAdmin, CLogin, CMove, ClientMessage, Health, MapLayout, PlayerGeneration, PlayerId, PlayerMoveIntent,
        PlayerMovementState, Position, ServerMessage,
    },
};

fn log_in(app: &mut App, name: &str) -> (Sender<ClientMessage>, Receiver<ServerMessage>) {
    let (client, receiver) = connect(app);
    client
        .send(ClientMessage::Login(CLogin { name: name.into() }))
        .expect("login failed");
    app.update();
    (client, receiver)
}

fn admin(client: &Sender<ClientMessage>, command: &str) {
    client
        .send(ClientMessage::Admin(CAdmin {
            command: command.into(),
        }))
        .expect("admin command delivery failed");
}

fn body(app: &App, id: PlayerId) -> Entity {
    app.world()
        .resource::<PlayerMap>()
        .get(&id)
        .expect("logged-in player missing")
        .entity()
        .expect("player has no body")
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
    let starting_at = |checkpoint: u32| ServerAppOptions {
        checkpoint: Some(checkpoint),
        ..options()
    };
    let error = server_app_with_options(starting_at(7), None)
        .expect_err("unknown checkpoint accepted")
        .to_string();
    assert!(
        error.contains("unknown checkpoint 7") && error.contains("checkpoints are 0, 1"),
        "{error}"
    );

    let mut app = server_app_with_options(starting_at(1), None).expect("server app failed to initialize");
    let (_, receiver) = log_in(&mut app, "Player");
    let relocation = receiver
        .try_iter()
        .find_map(|message| match message {
            ServerMessage::PlayerRelocated(relocation) => Some(relocation),
            _ => None,
        })
        .expect("login relocation missing");
    let pos = relocation.player.movement.pos;
    assert!(in_checkpoint(&app, 1, &pos), "{pos:?} is outside checkpoint 1");
    assert_eq!(
        app.world()
            .resource::<PlayerMap>()
            .get(&PlayerId(1))
            .expect("logged-in player missing")
            .session
            .checkpoint
            .number,
        1
    );
}

#[test]
fn return_command_relocates_the_living_sender_to_its_checkpoint_without_a_death() {
    let mut app = server_app(NetworkOverrides::default()).expect("server app failed to initialize");
    let (client, receiver) = log_in(&mut app, "Player");
    while receiver.try_recv().is_ok() {}
    let id = PlayerId(1);
    let entity = body(&app, id);
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
    admin(&client, "/return");
    app.update();
    let messages: Vec<_> = receiver.try_iter().collect();
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
    assert_eq!(body(&app, id), entity, "the body stays; only its generation advances");
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
    let cell_size = app.world().resource::<MapConfig>().grids[0].geometry.cell_size();
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
            info.life.movement = PlayerMovementState::new(pos, PlayerMoveIntent::NONE, 0.0, 0.0);
            info.entity().expect("body missing")
        };
        app.world_mut().entity_mut(entity).insert(pos);
    }
    for client in [&first, &second] {
        admin(client, "/return");
    }
    app.update();

    let positions =
        [PlayerId(1), PlayerId(2)].map(|id| *app.world().get::<Position>(body(&app, id)).expect("position missing"));
    assert_eq!(positions.iter().filter(|pos| in_checkpoint(&app, 0, pos)).count(), 1);
    assert_ne!(positions[0], positions[1], "both returns claimed the only spot");
    let relocations = receiver
        .try_iter()
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
        let (client, receiver) = log_in(&mut app, "Player");
        while receiver.try_recv().is_ok() {}
        let entity = body(&app, PlayerId(1));
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
            admin(&client, command);
        }
        app.update();
        assert_eq!(app.world().get::<Health>(entity).expect("health missing").0, max_health);
        let relocation_health: Vec<_> = receiver
            .try_iter()
            .filter_map(|message| match message {
                ServerMessage::PlayerRelocated(message) => Some(message.player.health.0),
                _ => None,
            })
            .collect();
        assert_eq!(relocation_health, expected_relocations, "{commands:?}");
    }
}

#[test]
fn initial_spawn_override_places_the_first_single_player_body_exactly() {
    let expected = Position {
        x: -8.5,
        y: 3.25,
        z: 11.0,
    };
    let spawning = ServerAppOptions {
        initial_spawn: Some(expected),
        ..options()
    };
    let mut app = server_app_with_options(spawning, None).expect("server app failed to initialize");
    let (_, receiver) = log_in(&mut app, "Player");

    let snapshot = receiver
        .try_iter()
        .find_map(|message| match message {
            ServerMessage::Snapshot(snapshot) => Some(snapshot),
            _ => None,
        })
        .expect("initial snapshot missing");
    let player = snapshot.players.first().expect("spawned player missing");
    assert_eq!(player.1.movement.pos, expected);
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
            let entity = body(&app, *id);
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
                movement: PlayerMovementState::new(*pos, PlayerMoveIntent::NONE, 0.0, 0.0),
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
    let latest_moves = |receiver: &Receiver<ServerMessage>| {
        receiver
            .try_iter()
            .filter_map(|message| match message {
                ServerMessage::PlayerMoves(moves) => Some(moves),
                _ => None,
            })
            .last()
            .expect("movement broadcast missing")
    };
    for (id, _, receiver) in &clients {
        let latest = latest_moves(receiver);
        assert_eq!(latest.moves.len(), 1);
        assert!(latest.moves.iter().all(|entry| entry.id != *id && entry.seq == 1));
    }
    app.update();
    for (id, _, receiver) in &clients {
        let latest = latest_moves(receiver);
        assert!(latest.moves.iter().all(|entry| entry.id != *id && entry.seq == 1));
    }
}

#[test]
fn rate_overrides_are_checked_together_before_app_creation() {
    for server_hz in [1_000_000_001, u32::MAX] {
        let error = server_app(NetworkOverrides {
            server_hz: Some(server_hz),
            ..default()
        })
        .expect_err("zero-duration server tick accepted")
        .to_string();
        assert!(error.contains("network.server_hz"), "{error}");
        assert!(error.contains("at least 1 ns"), "{error}");
    }
    for (server, updates, snapshots) in [(0, 1, 1), (30, 60, 4), (60, 30, 61)] {
        assert!(
            server_app(NetworkOverrides {
                server_hz: Some(server),
                update_hz: Some(updates),
                snapshot_hz: Some(snapshots),
            })
            .is_err()
        );
    }
}

#[test]
fn rate_overrides_reach_init_and_drive_the_tick_and_broadcast_cadences() {
    for (overrides, expected_moves, expected_snapshots) in [
        (
            NetworkOverrides {
                update_hz: Some(2),
                snapshot_hz: Some(7),
                ..default()
            },
            4,
            14,
        ),
        (
            NetworkOverrides {
                server_hz: Some(60),
                update_hz: Some(30),
                snapshot_hz: Some(4),
            },
            30,
            4,
        ),
    ] {
        let mut app = server_app(overrides).expect("server app failed");
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
        let init = receiver
            .try_iter()
            .find_map(|message| match message {
                ServerMessage::Init(init) => Some(init),
                _ => None,
            })
            .expect("init missing");
        let network = init.world.network;
        assert_eq!(Some(network.update_hz), overrides.update_hz);
        assert_eq!(Some(network.snapshot_hz), overrides.snapshot_hz);
        if let Some(server_hz) = overrides.server_hz {
            assert_eq!(network.server_hz, server_hz);
        }
        assert_eq!(app.world().resource::<NetworkConfig>().server_hz, network.server_hz);
        assert_eq!(app.world().resource::<NetworkConfig>().update_hz, network.update_hz);
        while receiver.try_recv().is_ok() {}
        let start_tick = app.world().resource::<ServerTick>().0;
        let start_time = app.world().resource::<Time>().elapsed_secs();
        for _ in 0..60 {
            app.update();
        }
        assert_eq!(app.world().resource::<ServerTick>().0.wrapping_sub(start_tick), 60);
        let elapsed = app.world().resource::<Time>().elapsed_secs() - start_time;
        assert!((elapsed - 60.0 / network.server_hz as f32).abs() < 1e-5);
        let mut moves = 0;
        let mut snapshots = 0;
        for message in receiver.try_iter() {
            match message {
                ServerMessage::PlayerMoves(_) => moves += 1,
                ServerMessage::Snapshot(_) => snapshots += 1,
                _ => {}
            }
        }
        assert_eq!(moves, expected_moves);
        assert_eq!(snapshots, expected_snapshots);
    }
}
