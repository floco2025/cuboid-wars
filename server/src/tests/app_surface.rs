use super::fixtures::connect;
use super::*;
use crate::actors::{SurfaceAgent, navigation::surface::fixtures};
use common::{
    physics::CharacterSupport,
    protocol::{CLogin, CMove, ClientMessage, Health, PlayerId, PlayerMoveIntent, PlayerMovementState, ServerMessage},
};

#[test]
fn ordinary_surface_actor_pursues_under_an_authored_plank_and_respawns() {
    let config = fixtures::config();
    let physics = config.expect_actor("scuttler").character.physics();
    let options = ServerAppOptions {
        map: None,
        god: true,
        peace: false,
        initial_spawn: None,
        checkpoint: None,
        logging: false,
        network: NetworkOverrides {
            server_hz: Some(30),
            update_hz: Some(30),
            snapshot_hz: Some(4),
        },
    };
    let mut app =
        build_server_app_with_loader(config, options, None, None, fixtures::generate).expect("ordinary authored app");
    let (client, receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin {
            name: "Observer".into(),
        }))
        .expect("login");
    app.update();
    let player = app.world().resource::<PlayerMap>().get(&PlayerId(1)).expect("player");
    let body = player.entity().expect("player body");
    let target = Position { x: 1.5, y: 0.0, z: 7.5 };
    let movement = PlayerMovementState::new(target, PlayerMoveIntent::Idle, 5.0, 0.0);
    client
        .send(ClientMessage::Move(CMove {
            generation: player.session.generation,
            seq: 1,
            portal_crossing: 0,
            movement,
        }))
        .expect("owner report");
    let (id, entity) = app
        .world()
        .resource::<ActorMap>()
        .iter()
        .map(|(id, info)| (*id, info.entity))
        .next()
        .expect("normal zone spawn");
    assert!(app.world().get::<SurfaceAgent>(entity).is_some());
    let mut passed_beneath = false;
    let mut approached = false;
    let mut broadcast = false;
    for _ in 0..600 {
        app.update();
        assert_eq!(*app.world().get::<Position>(body).expect("owner body"), target);
        assert_eq!(
            app.world()
                .resource::<PlayerMap>()
                .get(&PlayerId(1))
                .expect("owner")
                .life
                .movement
                .vertical_velocity,
            5.0
        );
        for message in receiver.try_iter() {
            if let ServerMessage::ActorMoves(moves) = message {
                broadcast |= moves
                    .moves
                    .iter()
                    .any(|entry| entry.id == id && entry.movement.support == CharacterSupport::Ground);
            }
        }
        let position = *app
            .world()
            .get::<Position>(entity)
            .expect("pursuer disappeared before approaching target");
        assert!(
            !app.world()
                .resource::<CollisionWorld>()
                .character_penetrates_solid(&position, physics, &[])
        );
        passed_beneath |=
            (-2.0..2.0).contains(&position.z) && (0.0..3.0).contains(&position.x) && position.y.abs() < 0.1;
        if position.distance_sq(&target) < 4.0 {
            approached = true;
            break;
        }
    }
    let state = app.world().get::<SurfaceAgent>(entity).expect("surface actor");
    assert!(
        passed_beneath && approached,
        "goal {:?}, failure {:?}, movement {:?}",
        state.goal,
        state.failure,
        state.executor
    );
    assert!(broadcast);
    assert!(state.route_revision.is_some());
    let before = *app.world().get::<Position>(entity).expect("before impulse");
    app.world_mut().entity_mut(entity).insert((
        common::physics::CharacterVerticalVelocity(5.0),
        common::physics::KnockbackVelocity(Vec3::X * 3.0),
    ));
    app.update();
    let after = *app.world().get::<Position>(entity).expect("after impulse");
    assert!(
        after.y > before.y + 0.1 && after.x > before.x + 0.05,
        "{before:?} -> {after:?}"
    );
    app.world_mut().get_mut::<Health>(entity).expect("actor health").0 = 0.0;
    for _ in 0..15 {
        app.update();
    }
    let actors = app.world().resource::<ActorMap>();
    assert!(actors.get(&id).is_none());
    let (_, replacement) = actors.iter().next().expect("normal respawn");
    assert_ne!(replacement.entity, entity);
    assert!(app.world().get::<SurfaceAgent>(replacement.entity).is_some());
    assert_eq!(
        *app.world().get::<Position>(body).expect("owner after actor respawn"),
        target
    );
}

#[test]
fn hotel_and_obby_run_surface_actors_through_normal_switches_and_replication() {
    use std::time::Instant;

    for name in ["hotel", "obby"] {
        let build_start = Instant::now();
        let mut app = build_server_app(
            ServerAppOptions {
                map: Some(name.into()),
                god: true,
                peace: true,
                initial_spawn: None,
                checkpoint: None,
                logging: false,
                network: NetworkOverrides::default(),
            },
            None,
            None,
        )
        .expect("shipped map surface launch");
        eprintln!("{name}: build {:?}", build_start.elapsed());
        let (client, receiver) = connect(&mut app);
        client
            .send(ClientMessage::Login(CLogin {
                name: "Observer".into(),
            }))
            .expect("login");
        app.update();
        let plates = app
            .world()
            .resource::<common::protocol::MapLayout>()
            .pressure_plates
            .clone();
        for (index, plate) in plates.iter().enumerate() {
            let player = app.world().resource::<PlayerMap>().get(&PlayerId(1)).expect("observer");
            let mut movement = PlayerMovementState::new(
                Position {
                    x: plate.center_x,
                    y: plate.center_y + 0.1,
                    z: plate.center_z,
                },
                PlayerMoveIntent::Idle,
                0.0,
                0.0,
            );
            movement.carrier = plate.carrier;
            client
                .send(ClientMessage::Move(CMove {
                    generation: player.session.generation,
                    seq: index as u32 + 1,
                    portal_crossing: 0,
                    movement,
                }))
                .expect("visit plate");
            app.update();
            app.update();
        }
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        while app
            .world()
            .resource::<crate::actors::navigation::surface::SurfaceNavigation>()
            .rebuilding()
        {
            assert!(Instant::now() < deadline, "{name}: navigation rebuild timed out");
            std::thread::sleep(std::time::Duration::from_millis(1));
            app.update();
        }
        let mut samples = Vec::new();
        let mut surface_samples = 0;
        let mut moving_samples = 0;
        let mut replicated = false;
        for _ in 0..600 {
            let start = Instant::now();
            app.update();
            samples.push(start.elapsed().as_micros());
            let world = app.world();
            let fields = &world.resource::<common::protocol::SwitchState>().open_fields;
            for info in world.resource::<ActorMap>().values() {
                let character = world
                    .get::<crate::actors::ActorCharacter>(info.entity)
                    .expect("actor body");
                if character.0.flies() || character.0.immovable {
                    continue;
                }
                let agent = world
                    .get::<SurfaceAgent>(info.entity)
                    .expect("ground actor uses surfaces");
                surface_samples += 1;
                let position = world.get::<Position>(info.entity).expect("actor position");
                assert!(Vec3::from(*position).is_finite());
                assert!(
                    !world.resource::<CollisionWorld>().character_penetrates_solid(
                        position,
                        character.0.physics(),
                        fields
                    ),
                    "{name}: actor {:?} at {position:?}",
                    info.spawn_kind
                );
                moving_samples += usize::from(
                    agent
                        .executor
                        .as_ref()
                        .is_some_and(|executor| executor.intent.speed().is_some_and(|speed| speed > 0.0)),
                );
            }
            for message in receiver.try_iter() {
                replicated |= matches!(message, ServerMessage::ActorMoves(_));
            }
        }
        samples.sort_unstable();
        let mut outcomes = std::collections::BTreeMap::new();
        for info in app.world().resource::<ActorMap>().values() {
            if let Some(agent) = app.world().get::<SurfaceAgent>(info.entity) {
                *outcomes
                    .entry(format!(
                        "{} {:?} {:?}",
                        info.spawn_kind,
                        agent.failure,
                        agent.executor.as_ref().map(|executor| executor.status)
                    ))
                    .or_insert(0) += 1;
            }
        }
        eprintln!("{name}: final actor outcomes {outcomes:?}");
        eprintln!(
            "{name}: full ticks p50={}us p95={}us p99={}us max={}us; surface samples={surface_samples}, moving={moving_samples}",
            samples[300], samples[570], samples[594], samples[599]
        );
        assert!(
            surface_samples > 0 && moving_samples > 0 && replicated,
            "{name}: ordinary surface actors did not run"
        );
    }
}

#[test]
fn ordinary_surface_actor_pursues_a_player_using_an_authored_shuttle() {
    let config = fixtures::config();
    let mut app = build_server_app_with_loader(
        config,
        ServerAppOptions {
            map: None,
            god: true,
            peace: false,
            initial_spawn: None,
            checkpoint: None,
            logging: false,
            network: NetworkOverrides::default(),
        },
        None,
        None,
        |_, hz, settings| fixtures::compile(fixtures::shuttle(), hz, settings),
    )
    .expect("shuttle app");
    let (client, receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin {
            name: "Observer".into(),
        }))
        .expect("login");
    app.update();
    let generation = app
        .world()
        .resource::<PlayerMap>()
        .get(&PlayerId(1))
        .expect("player")
        .session
        .generation;
    let target = Position {
        x: 8.0,
        y: 0.0,
        z: -1.5,
    };
    client
        .send(ClientMessage::Move(CMove {
            generation,
            seq: 1,
            portal_crossing: 0,
            movement: PlayerMovementState::new(target, PlayerMoveIntent::Idle, 0.0, 0.0),
        }))
        .expect("goal on island");
    let entity = app
        .world()
        .resource::<ActorMap>()
        .values()
        .next()
        .expect("pursuer")
        .entity;
    let mut rode = false;
    let mut approached = false;
    for _ in 0..1200 {
        app.update();
        for message in receiver.try_iter() {
            if let ServerMessage::ActorMoves(moves) = message {
                rode |= moves.moves.iter().any(|entry| !entry.movement.carrier.is_world());
            }
        }
        let position = *app.world().get::<Position>(entity).expect("pursuer disappeared");
        if position.distance_sq(&target) < 4.0 {
            approached = true;
            break;
        }
    }
    let agent = app.world().get::<SurfaceAgent>(entity).expect("pursuer state");
    assert!(
        rode && approached,
        "goal {:?}, failure {:?}, executor {:?}",
        agent.goal,
        agent.failure,
        agent.executor
    );
}

#[test]
fn ordinary_surface_actors_take_turns_on_a_ladder_in_opposite_directions() {
    let mut config = fixtures::config();
    config
        .actors
        .get_mut("scuttler")
        .expect("climber")
        .character
        .can_use_ladders = true;
    let mut app = build_server_app_with_loader(config, ServerAppOptions {
        map: None,  god: true, peace: true, initial_spawn: None,
        checkpoint: None, logging: false, network: NetworkOverrides::default(),
    }, None, None, |_,hz,settings| {
        let floor = |col,row| serde_json::json!({"col":col,"row":row,"all":"basement-floor"});
        fixtures::compile(serde_json::json!({"map": {
            "grid_cols":4,"grid_rows":4,"fireworks":null,
            "levels":[
                {"floors":(0..4).flat_map(|row| (2..4).map(move |col| floor(col,row))).collect::<Vec<_>>()},
                {},
                {"floors":(0..4).flat_map(|row| (0..2).map(move |col| floor(col,row))).collect::<Vec<_>>()}
            ],
            "ladders":[{"lower_level":0,"col":1,"row":1,"side":"E","levels":2}],
            "checkpoints":[{"level":0,"cols":[3,4],"rows":[2,3],"number":0,"type":"individual"}],
            "actor_spawn_zones":[{"level":0,"cols":[2,3],"rows":[1,2],"kind":"scuttler","count":[2],"respawn_secs":null}]
        }}),hz,settings)
    }).expect("ladder app");
    let (client, _receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin {
            name: "Observer".into(),
        }))
        .expect("login");
    app.update();
    let entities: Vec<_> = app
        .world()
        .resource::<ActorMap>()
        .values()
        .map(|info| info.entity)
        .collect();
    assert_eq!(entities.len(), 2);
    let lower = Position {
        x: 2.0,
        y: 0.0,
        z: -1.5,
    };
    let upper = Position {
        x: -2.0,
        y: 3.0,
        z: -1.5,
    };
    for (entity, start, goal) in [(entities[0], lower, upper), (entities[1], upper, lower)] {
        *app.world_mut().get_mut::<Position>(entity).expect("body position") = start;
        let mut agent = app
            .world_mut()
            .get_mut::<SurfaceAgent>(entity)
            .expect("surface controller");
        agent.goal = Some(crate::actors::SurfaceGoal {
            carrier: common::protocol::CarrierId::WORLD,
            position: goal,
        });
        agent.decision_secs = f32::MAX;
        agent.executor = None;
    }
    let mut completed = false;
    let mut climbing = 0;
    for _ in 0..900 {
        app.update();
        let occupants = entities
            .iter()
            .filter(|&&entity| {
                *app.world().get::<CharacterSupport>(entity).expect("climber support") == CharacterSupport::Ladder
            })
            .count();
        assert!(occupants <= 1, "opposing actors occupied the ladder together");
        climbing += occupants;
        if entities
            .iter()
            .all(|&entity| app.world().get::<SurfaceAgent>(entity).expect("climber").reached())
        {
            completed = true;
            break;
        }
    }
    assert!(
        completed && climbing > 0,
        "opposing climbers did not complete their routes"
    );
}

#[test]
fn surface_actor_roams_inside_a_small_home_on_a_large_surface() {
    let config = fixtures::config();
    let mut app = build_server_app_with_loader(config, ServerAppOptions {
        map: None,  god: true, peace: true, initial_spawn: None,
        checkpoint: None, logging: false, network: NetworkOverrides::default(),
    }, None, None, |_,hz,settings| fixtures::compile(serde_json::json!({"map": {
        "grid_cols":4,"grid_rows":4,"fireworks":null,
        "levels":[{"floors":(0..4).flat_map(|row| (0..4).map(move |col| serde_json::json!({"col":col,"row":row,"all":"basement-floor"}))).collect::<Vec<_>>()}],
        "checkpoints":[{"level":0,"cols":[3,4],"rows":[3,4],"number":0,"type":"individual"}],
        "actor_spawn_zones":[{"level":0,"cols":[0,1],"rows":[0,1],"kind":"scuttler","count":[1],"respawn_secs":null}]
    }}),hz,settings)).expect("small home app");
    let (client, _receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin {
            name: "Observer".into(),
        }))
        .expect("login");
    app.update();
    let entity = app
        .world()
        .resource::<ActorMap>()
        .values()
        .next()
        .expect("actor")
        .entity;
    let start = *app.world().get::<Position>(entity).expect("initial position");
    let mut roamed = false;
    for _ in 0..120 {
        app.update();
        let position = *app.world().get::<Position>(entity).expect("roaming actor");
        roamed |= position.distance_sq(&start) > 0.25;
        assert!(
            (-6.0..=-3.0).contains(&position.x) && (-6.0..=-3.0).contains(&position.z),
            "left home: {position:?}"
        );
    }
    assert!(roamed, "large polygon provided no goal inside the small home");
}

#[test]
fn wide_ground_actor_flees_over_hills_and_resumes_pursuit_on_the_physical_terrain() {
    use serde_json::json;
    let mut config = fixtures::config();
    config.settings.grounds = Some(common::map::GroundsSettings { level: 0 });
    config.power_ups.single_shot = crate::config::PowerUpMode::Always {};
    let actor = &mut config.actors.get_mut("bruiser").expect("body").character;
    actor.character.movement_collider.diameter = 1.6;
    actor.character.movement_collider.height = 1.7;
    actor.character.hitbox = common::config::HitboxConfig {
        width: 1.6,
        height: 1.7,
        depth: 1.6,
        bottom_offset: 0.0,
    };
    actor.character.eye_height = 1.3;
    config
        .settings
        .movement
        .actors
        .get_mut("bruiser")
        .expect("speed")
        .active_speed = 8.0;
    let mut app = build_server_app_with_loader(
        config,
        ServerAppOptions { map: None,  god: true, peace: false,
            initial_spawn: None, checkpoint: None, logging: false, network: NetworkOverrides::default() },
        None, None,
        // Leave vertical bake room for the surrounding rolling grounds.
        |_, hz, settings| fixtures::compile(json!({"map": {
            "grid_cols":24,"grid_rows":24,"fireworks":null,
            "levels":[{"terrain":[{"col":11,"row":11,"all":"basement-floor"},{"col":12,"row":11,"all":"basement-floor"},
                {"col":11,"row":12,"all":"basement-floor"},{"col":12,"row":12,"all":"basement-floor"}]},{},{},{},{"inaccessible_floors":[{"col":10,"row":10,"all":"basement-floor"}]},{}],
            "checkpoints":[{"level":0,"cols":[11,12],"rows":[11,12],"number":0,"type":"individual"}],
            "actor_spawn_zones":[{"level":0,"cols":[12,13],"rows":[12,13],"kind":"bruiser", "count":[1],"respawn_secs":null}]
        }}), hz, settings),
    ).expect("terrain scene");
    let (client, _receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin {
            name: "Observer".into(),
        }))
        .expect("login");
    app.update();
    let (id, entity) = app
        .world()
        .resource::<ActorMap>()
        .iter()
        .map(|(id, actor)| (*id, actor.entity))
        .next()
        .expect("wide actor");
    let generation = app
        .world()
        .resource::<PlayerMap>()
        .get(&PlayerId(1))
        .expect("owner")
        .session
        .generation;
    let start = *app.world().get::<Position>(entity).expect("start");
    let mut seq = 0;
    let report = |client: &crossbeam_channel::Sender<ClientMessage>, seq, pos| {
        let mut movement = PlayerMovementState::new(pos, PlayerMoveIntent::Idle, 0.0, 0.0);
        movement.support = CharacterSupport::Ground;
        client
            .send(ClientMessage::Move(CMove {
                generation,
                seq,
                portal_crossing: 0,
                movement,
            }))
            .expect("owner report");
    };
    let mut fled = false;
    for _ in 0..600 {
        let pos = *app.world().get::<Position>(entity).expect("fleeing actor");
        seq += 1;
        report(
            &client,
            seq,
            Position {
                // Standing on the inaccessible roof, not briefly airborne.
                x: -3.1,
                y: 6.0,
                z: -3.1,
            },
        );
        app.update();
        fled |= matches!(
            app.world().resource::<ActorMap>().get(&id).expect("actor").mode,
            crate::actors::ActorMode::Evade { .. }
        );
        if pos.horizontal_distance_sq(&start) > 25.0_f32.powi(2) {
            break;
        }
    }
    let after = *app.world().get::<Position>(entity).expect("hill position");
    let agent = app.world().get::<SurfaceAgent>(entity).expect("agent");
    assert!(
        fled && after.horizontal_distance_sq(&start) > 24.0_f32.powi(2) && after.y > start.y + 1.0,
        "flee stopped {start:?} -> {after:?}; {:?} {:?}",
        agent.failure,
        agent.executor
    );
    let target = app
        .world()
        .resource::<CollisionWorld>()
        .support_surface_on_carrier(
            Vec3::new(after.x - 6.0, 20.0, after.z - 6.0),
            40.0,
            common::protocol::CarrierId::WORLD,
            &[],
        )
        .expect("player's terrain")
        .point;
    seq += 1;
    report(&client, seq, (target + Vec3::Y * 0.02).into());
    let mut engaged = false;
    for _ in 0..180 {
        app.update();
        engaged |= matches!(
            app.world().resource::<ActorMap>().get(&id).expect("pursuer").mode,
            crate::actors::ActorMode::Engage { .. }
        );
        let pos = *app.world().get::<Position>(entity).expect("pursuer position");
        if engaged && pos.horizontal_distance_sq(&target.into()) < 9.0 {
            break;
        }
    }
    let end = *app.world().get::<Position>(entity).expect("final actor");
    let agent = app.world().get::<SurfaceAgent>(entity).expect("agent");
    assert!(
        engaged && end.horizontal_distance_sq(&target.into()) < 9.0,
        "pursuit stopped {after:?} -> {end:?}, target {target:?}; {:?} {:?}",
        agent.failure,
        agent.executor
    );
}

#[test]
fn surface_actor_pursues_across_navigation_regions_then_returns_home() {
    for (diameter, height) in [(0.8, 1.8), (1.6, 1.7)] {
        let mut config = fixtures::config();
        config.settings.grounds = Some(common::map::GroundsSettings { level: 0 });
        config.power_ups.single_shot = crate::config::PowerUpMode::Always {};
        let actor = config.actors.get_mut("bruiser").expect("pursuer config");
        actor.character.character.movement_collider.diameter = diameter;
        actor.character.character.movement_collider.height = height;
        actor.character.character.hitbox = common::config::HitboxConfig {
            width: diameter,
            height,
            depth: diameter,
            bottom_offset: 0.0,
        };
        actor.character.character.eye_height = 1.3;
        actor.vision_range = 80.0;
        actor.threat_memory_secs = 5.0;
        config
            .settings
            .movement
            .actors
            .get_mut("bruiser")
            .expect("speed")
            .active_speed = 8.0;
        config
            .settings
            .movement
            .actors
            .get_mut("bruiser")
            .expect("speed")
            .roam_speed = 8.0;
        let physics = config.expect_actor("bruiser").character.physics();
        let mut app = build_server_app_with_loader(config, ServerAppOptions {
        map: None,  god: true, peace: false, initial_spawn: None,
        checkpoint: None, logging: false, network: NetworkOverrides::default(),
    }, None, None, |_, hz, settings| fixtures::compile(serde_json::json!({"map": {
        "grid_cols":4,"grid_rows":4,"fireworks":null,
        "levels":[{"terrain":(1..3).flat_map(|row| (1..3).map(move |col| serde_json::json!({"col":col,"row":row,"all":"basement-floor"}))).collect::<Vec<_>>()}],
        "checkpoints":[{"level":0,"cols":[1,2],"rows":[1,2],"number":0,"type":"individual"}],
        "actor_spawn_zones":[{"level":0,"cols":[2,3],"rows":[2,3],"kind":"bruiser","count":[1],"respawn_secs":null}]
    }}), hz, settings)).expect("open world scene");
        let (client, _receiver) = connect(&mut app);
        client
            .send(ClientMessage::Login(CLogin {
                name: "Observer".into(),
            }))
            .expect("login");
        app.update();
        let (id, entity) = app
            .world()
            .resource::<ActorMap>()
            .iter()
            .map(|(id, actor)| (*id, actor.entity))
            .next()
            .expect("pursuer");
        let generation = app
            .world()
            .resource::<PlayerMap>()
            .get(&PlayerId(1))
            .expect("owner")
            .session
            .generation;
        let start = *app.world().get::<Position>(entity).expect("start");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut crossed = false;
        for seq in 1..6001 {
            assert!(std::time::Instant::now() < deadline, "streaming chase timed out");
            let pos = *app.world().get::<Position>(entity).expect("pursuer");
            assert!(
                !app.world()
                    .resource::<CollisionWorld>()
                    .character_penetrates_solid(&pos, physics, &[]),
                "pursuit penetrated terrain: {pos:?}"
            );
            let target = app
                .world()
                .resource::<CollisionWorld>()
                .support_surface_on_carrier(
                    Vec3::new(pos.x + 16.0, 100.0, 1.5),
                    150.0,
                    common::protocol::CarrierId::WORLD,
                    &[],
                )
                .expect("distant terrain")
                .point;
            client
                .send(ClientMessage::Move(CMove {
                    generation,
                    seq,
                    portal_crossing: 0,
                    movement: PlayerMovementState::new(
                        (target + Vec3::Y * 0.02).into(),
                        PlayerMoveIntent::Idle,
                        0.0,
                        0.0,
                    ),
                }))
                .expect("moving owner");
            app.update();
            if pos.x > 220.0 {
                crossed = true;
                break;
            }
            if app
                .world()
                .resource::<crate::actors::navigation::surface::SurfaceNavigation>()
                .rebuilding()
            {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        let far = *app.world().get::<Position>(entity).expect("distant actor");
        let agent = app.world().get::<SurfaceAgent>(entity).expect("distant navigation");
        assert!(
            crossed,
            "pursuit stopped at {far:?}: {:?} {:?}, mode {:?}",
            agent.failure,
            agent.executor,
            app.world().resource::<ActorMap>().get(&id).expect("pursuer").mode
        );
        app.world_mut().resource_mut::<ActorMap>().peaceful = true;
        let mut returned = false;
        for _ in 0..6000 {
            app.update();
            let pos = *app.world().get::<Position>(entity).expect("returning actor");
            assert!(
                !app.world()
                    .resource::<CollisionWorld>()
                    .character_penetrates_solid(&pos, physics, &[]),
                "return penetrated terrain: {pos:?}"
            );
            if pos.horizontal_distance_sq(&start) < 9.0 {
                returned = true;
                break;
            }
            if app
                .world()
                .resource::<crate::actors::navigation::surface::SurfaceNavigation>()
                .rebuilding()
            {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        let end = *app.world().get::<Position>(entity).expect("actor home");
        let agent = app.world().get::<SurfaceAgent>(entity).expect("home navigation");
        assert!(
            returned,
            "return stopped at {end:?}: {:?} {:?}",
            agent.failure, agent.executor
        );
    }
}

#[test]
fn moving_player_pursuit_stays_direct_past_an_off_path_obstacle() {
    use common::{
        math::angle_delta_radians,
        protocol::{FaceYaw, Wall},
    };
    use std::f32::consts::{FRAC_PI_2, TAU};
    let config = fixtures::config();
    let physics = config.expect_actor("scuttler").character.physics();
    let mut app = build_server_app_with_loader(config, ServerAppOptions {
        map: None,  god: true, peace: false, initial_spawn: None,
        checkpoint: None, logging: false,
        network: NetworkOverrides { server_hz: Some(30), update_hz: Some(30), snapshot_hz: Some(4) },
    }, None, None, |_, hz, settings| {
        let mut generated = fixtures::compile(serde_json::json!({"map": {
            "grid_cols":20,"grid_rows":6,"fireworks":null,
            "levels":[{"floors":(0..6).flat_map(|row| (0..20).map(move |col| serde_json::json!({"col":col,"row":row,"all":"basement-floor"}))).collect::<Vec<_>>()}],
            "checkpoints":[{"level":0,"cols":[2,3],"rows":[1,2],"number":0,"type":"individual"}],
            "actor_spawn_zones":[{"level":0,"cols":[2,3],"rows":[1,2],"kind":"scuttler","count":[1],"respawn_secs":null}]
        }}), hz, settings)?;
        generated.layout.walls.push(Wall { x1: 0.0, x2: 0.0, z1: -1.0, z2: 1.0, width: 2.0, y: 0.0, height: 3.0, level: 0, carrier: common::protocol::CarrierId::WORLD });
        Ok(generated)
    }).expect("pursuit scene");
    let (client, _receiver) = connect(&mut app);
    client
        .send(ClientMessage::Login(CLogin {
            name: "Observer".into(),
        }))
        .expect("login");
    app.update();
    let player = app.world().resource::<PlayerMap>().get(&PlayerId(1)).expect("owner");
    let generation = player.session.generation;
    let body = player.entity().expect("owner body");
    let entity = app
        .world()
        .resource::<ActorMap>()
        .values()
        .next()
        .expect("actor")
        .entity;
    let start = Position {
        x: -22.5,
        y: 0.0,
        z: -4.5,
    };
    app.world_mut()
        .entity_mut(entity)
        .insert((start, FaceYaw(FRAC_PI_2), SurfaceAgent::default()));
    let mut before = start;
    let mut heading = FRAC_PI_2;
    for tick in 1..=180 {
        let target = Position {
            x: -6.5 + tick as f32 * 0.025,
            y: 0.0,
            z: -4.5 + (tick as f32 * 0.025).sin() * 0.3,
        };
        client
            .send(ClientMessage::Move(CMove {
                generation,
                seq: tick,
                portal_crossing: 0,
                movement: PlayerMovementState::new(target, PlayerMoveIntent::Idle, 0.0, 0.0),
            }))
            .expect("moving owner");
        app.update();
        assert_eq!(*app.world().get::<Position>(body).expect("owner body"), target);
        let position = *app.world().get::<Position>(entity).expect("pursuer");
        let facing = app.world().get::<FaceYaw>(entity).expect("heading").0;
        assert!(
            angle_delta_radians(facing, heading).abs() <= TAU / 30.0 + 1e-5,
            "snapped heading at {tick}"
        );
        assert!(
            (position.z - start.z).abs() < 0.4,
            "sideways detour at {tick}: {position:?}"
        );
        assert!(
            position.x >= before.x - 0.001,
            "backward step at {tick}: {before:?} -> {position:?}"
        );
        assert!(
            !app.world()
                .resource::<CollisionWorld>()
                .character_penetrates_solid(&position, physics, &[])
        );
        before = position;
        heading = facing;
    }
    assert!(before.x > start.x + 16.0, "pursuit stopped: {before:?}");
}
