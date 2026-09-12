use crate::config::fixtures;
use crate::{
    actors::ActorMap,
    characters::characters_movement_system,
    network::collect_player_moves,
    players::{PlayerInfo, PlayerMap, PlayerStateQuery, apply_player_movement_system, queue_player_movement},
};
use bevy::{ecs::system::SystemState, prelude::*};
use common::{
    constants::TICK_DURATION,
    map::Carriers,
    physics::{CharacterSupport, CollisionWorld, PortalSet, knockback_decay_system},
    protocol::*,
};
use crossbeam_channel::unbounded;

const ID: PlayerId = PlayerId(1);

fn movement_app(layout: MapLayout) -> (App, Entity) {
    let config = fixtures::server_config();
    let mut app = App::new();
    let mut time = Time::<()>::default();
    time.advance_by(TICK_DURATION);
    app.insert_resource(time)
        .insert_resource(config.gameplay_config())
        .insert_resource(
            config
                .maps
                .get("hotel")
                .expect("hotel map missing from the server gameplay config")
                .settings
                .clone(),
        )
        .insert_resource(CollisionWorld::from_map_layout(&layout))
        .insert_resource(Carriers::from_layout(&layout))
        .init_resource::<PortalSet>()
        .init_resource::<PlayerMap>()
        .init_resource::<ActorMap>()
        .init_resource::<crate::actors::navigation::ActorTerritories>()
        .init_resource::<PlateState>()
        .init_resource::<ServerTick>()
        .add_systems(
            Update,
            (
                server_tick_advance_system,
                apply_player_movement_system,
                characters_movement_system,
                knockback_decay_system::<With<ActorMarker>>,
            )
                .chain(),
        );
    let entity = app
        .world_mut()
        .spawn((ID, PlayerMarker, Position::default(), FaceYaw(0.0), Health(100.0)))
        .id();
    let (sender, _) = unbounded();
    let mut info = PlayerInfo::new(entity, sender);
    info.connection.logged_in = true;
    app.world_mut().resource_mut::<PlayerMap>().insert(ID, info);
    (app, entity)
}

fn report(seq: u32, pos: Position) -> CMove {
    CMove {
        generation: PlayerGeneration(0),
        seq,
        portal_crossing: 0,
        movement: PlayerMovementState::new(pos, PlayerMoveIntent::Idle, 0.0, 0.0),
    }
}

fn deliver(app: &mut App, message: CMove) {
    app.world_mut().resource_scope(|world, mut players: Mut<PlayerMap>| {
        queue_player_movement(ID, message, &mut players, world.resource::<Carriers>());
    });
}

fn cross(app: &mut App, seq: u32, crossing: u32, exit_x: f32) {
    let mut message = report(seq, Position { x: exit_x, ..default() });
    message.portal_crossing = crossing;
    deliver(app, message);
}

fn player_info(app: &App) -> &PlayerInfo {
    app.world().resource::<PlayerMap>().get(&ID).expect("player missing")
}

#[test]
fn carrier_local_reports_relay_unchanged_while_the_server_places_the_rider_with_the_platform() {
    let layout = MapLayout {
        carriers: vec![Carrier {
            switch_inverted: false,

            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: Position {
                x: 100.0,
                y: 0.0,
                z: 0.0,
            },
            to: Position {
                x: 110.0,
                y: 5.0,
                z: 0.0,
            },
            travel_ticks: 30,
            pause_ticks: 10,
            phase_ticks: 0,
            switch: None,
        }],
        ..default()
    };
    let (mut app, entity) = movement_app(layout);
    let local = Position {
        x: 2.0,
        y: 0.01,
        z: 1.0,
    };
    let mut message = report(1, local);
    message.movement.carrier = CarrierId(1);
    message.movement.support = CharacterSupport::Ground;
    deliver(&mut app, message);
    for tick in [1, 15, 30, 40, 55, 70, 80] {
        app.world_mut()
            .resource_mut::<Carriers>()
            .advance(tick, &PlateState::default());
        app.update();
        let expected = app
            .world()
            .resource::<Carriers>()
            .pose(CarrierId(1))
            .transform_position(&local);
        assert_eq!(
            *app.world().get::<Position>(entity).expect("rider position missing"),
            expected
        );
        let relayed = result(&mut app);
        assert_eq!(relayed.seq, 1);
        assert_eq!(relayed.movement.carrier, CarrierId(1));
        assert_eq!(relayed.movement.pos, local);
    }
    let exit = Position {
        x: 112.0,
        y: 6.0,
        z: 1.0,
    };
    deliver(&mut app, report(2, exit));
    for tick in [81, 90, 100] {
        app.world_mut()
            .resource_mut::<Carriers>()
            .advance(tick, &PlateState::default());
        app.update();
        assert_eq!(
            *app.world().get::<Position>(entity).expect("player position missing"),
            exit
        );
        assert_eq!(result(&mut app).movement.carrier, CarrierId::WORLD);
    }
}

#[test]
fn coalesced_reports_retain_the_latest_crossing_marker() {
    let (mut app, _) = movement_app(MapLayout::default());
    deliver(&mut app, report(1, Position::default()));
    cross(&mut app, 2, 1, 100.0);
    cross(&mut app, 3, 1, 101.0);
    cross(&mut app, 4, 2, -100.0);
    cross(&mut app, 5, 2, -99.0);
    deliver(&mut app, report(1, Position::default()));
    app.update();
    let entry = result(&mut app);
    assert_eq!(entry.seq, 5);
    assert_eq!(entry.portal_crossing, 2);
    assert_eq!(entry.movement.pos.x, -99.0);
    app.update();
    assert_eq!(result(&mut app).portal_crossing, 2);
}

#[test]
fn body_replacement_discards_queued_and_in_flight_reports_without_advancing_the_cutoff() {
    for instant_respawn in [false, true] {
        let (mut app, entity) = movement_app(MapLayout::default());
        app.world_mut().resource_mut::<MapSettings>().movement.gravity = 0.0;
        deliver(&mut app, report(1, Position { x: 100.0, ..default() }));
        cross(&mut app, 2, 1, 200.0);
        {
            let mut players = app.world_mut().resource_mut::<PlayerMap>();
            let info = players.get_mut(&ID).expect("player missing");
            if instant_respawn {
                info.begin_respawn(0.0);
                info.finish_respawn(entity);
            } else {
                info.advance_body();
            }
        }
        deliver(&mut app, report(1000, Position { x: 200.0, ..default() }));
        cross(&mut app, 1001, 2, 300.0);
        app.update();
        let entry = result(&mut app);
        assert_eq!(entry.generation, PlayerGeneration(1));
        assert_eq!(entry.seq, 2);
        assert_eq!(entry.portal_crossing, 0);
        assert_eq!(entry.movement.pos.x, 0.0);
        assert_eq!(player_info(&app).session.last_move_seq, 2);
        let mut fresh = report(3, Position { x: 500.0, ..default() });
        fresh.generation = PlayerGeneration(1);
        deliver(&mut app, fresh);
        app.update();
        let entry = result(&mut app);
        assert_eq!(entry.seq, 3);
        assert_eq!(entry.movement.pos.x, 500.0);
    }
}

#[test]
fn crossing_and_full_exit_motion_survive_sequence_wrap() {
    let (mut app, _) = movement_app(MapLayout::default());
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&ID)
        .expect("player missing")
        .session
        .last_move_seq = u32::MAX - 1;
    cross(&mut app, u32::MAX, 1, 100.0);
    app.update();
    assert_eq!(result(&mut app).movement.pos.x, 100.0);
    cross(&mut app, u32::MAX, 2, 200.0);
    let mut movement = PlayerMovementState::new(
        Position {
            x: 100.0,
            y: 10.0,
            z: -20.0,
        },
        PlayerMoveIntent::Running { direction: 1.0 },
        12.0,
        -2.0,
    )
    .with_momentum(Vec3::X * 4.0, Vec3::Z * -3.0);
    movement.support = CharacterSupport::Ladder;
    deliver(
        &mut app,
        CMove {
            generation: PlayerGeneration(0),
            seq: 1,
            portal_crossing: 2,
            movement,
        },
    );
    app.update();
    let entry = result(&mut app);
    assert_eq!(entry.seq, 1);
    assert_eq!(entry.movement.pos, movement.pos);
    assert_eq!(entry.movement.move_intent, movement.move_intent);
    assert_eq!(entry.movement.face_yaw, movement.face_yaw);
    assert_eq!(entry.movement.vertical_velocity, movement.vertical_velocity);
    assert_eq!(entry.movement.airborne_momentum, movement.airborne_momentum);
    assert_eq!(entry.movement.knockback, movement.knockback);
    assert_eq!(entry.movement.support, movement.support);
}

fn result(app: &mut App) -> PlayerMove {
    let mut state = SystemState::<PlayerStateQuery>::new(app.world_mut());
    let positions = state.get(app.world()).expect("movement collection state invalid");
    collect_player_moves(app.world().resource::<PlayerMap>(), &positions)
        .into_iter()
        .find(|entry| entry.id == ID)
        .expect("movement missing")
}

#[test]
fn accepts_client_state_through_a_wall() {
    let layout = MapLayout {
        walls: vec![Wall {
            x1: 1.0,
            x2: 1.0,
            z1: -5.0,
            z2: 5.0,
            y: -1.0,
            height: 10.0,
            width: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    let (mut app, entity) = movement_app(layout);
    let mut message = report(1, Position { x: 2.0, y: 1.0, z: 0.0 });
    message.movement = PlayerMovementState::new(
        message.movement.pos,
        PlayerMoveIntent::Running { direction: 2.0 },
        7.0,
        2.0,
    )
    .with_momentum(Vec3::new(-3.0, 0.0, 2.0), Vec3::X);
    let expected = message.movement;
    deliver(&mut app, message);
    app.update();
    assert_eq!(
        *app.world().get::<Position>(entity).expect("position missing"),
        expected.pos
    );
    assert_eq!(app.world().get::<FaceYaw>(entity).expect("facing missing").0, 2.0);
    let relayed = result(&mut app).movement;
    assert_eq!(relayed.pos, expected.pos);
    assert_eq!(relayed.vertical_velocity, 7.0);
    assert_eq!(relayed.airborne_momentum, [-3.0, 0.0, 2.0]);
    assert_eq!(relayed.knockback, [1.0, 0.0, 0.0]);
    assert_eq!(relayed.move_intent, expected.move_intent);
}

#[test]
fn accepted_reports_remain_stationary_without_fresh_reports() {
    let (mut app, entity) = movement_app(MapLayout::default());
    deliver(
        &mut app,
        report(
            1,
            Position {
                x: 500.0,
                y: 0.0,
                z: 0.0,
            },
        ),
    );
    let mut moving = report(2, Position { x: 500.0, ..default() });
    moving.movement.vertical_velocity = -8.0;
    moving.movement.knockback = [3.0, 0.0, 2.0];
    moving.movement.move_intent = PlayerMoveIntent::Running { direction: 1.0 };
    deliver(&mut app, moving);
    app.update();
    let accepted = result(&mut app);
    assert_eq!(accepted.movement.pos.x, 500.0);
    for _ in 0..10 {
        app.update();
    }
    let repeated = result(&mut app);
    assert_eq!(accepted.seq, 2);
    assert_eq!(repeated.seq, 2);
    assert_eq!(repeated.movement.pos, accepted.movement.pos);
    assert_eq!(repeated.movement.vertical_velocity, accepted.movement.vertical_velocity);
    assert_eq!(repeated.movement.knockback, accepted.movement.knockback);
    assert_eq!(
        *app.world().get::<Position>(entity).expect("position missing"),
        repeated.movement.pos
    );
}

#[test]
fn only_newest_report_steers_and_is_processed_even_when_packets_arrive_together() {
    let (mut app, _) = movement_app(MapLayout::default());
    let mut stale = report(1, Position { x: 3.0, y: 0.0, z: 0.0 });
    stale.movement.move_intent = PlayerMoveIntent::Running { direction: 1.0 };
    deliver(&mut app, stale.clone());
    deliver(&mut app, report(3, Position { x: 1.0, y: 0.0, z: 0.0 }));
    deliver(&mut app, stale);
    app.update();
    let entry = result(&mut app);
    assert_eq!(entry.seq, 3);
    assert_eq!(entry.movement.pos.x, 1.0);
    assert_eq!(entry.movement.move_intent, PlayerMoveIntent::Idle);
}

#[test]
fn non_finite_reports_do_not_advance_sequence_or_replace_fresh_state() {
    let (mut app, _) = movement_app(MapLayout::default());
    deliver(&mut app, report(1, Position::default()));
    app.update();
    let mut invalid = report(101, Position::default());
    invalid.movement.knockback[0] = f32::NAN;
    deliver(&mut app, invalid);
    deliver(&mut app, report(2, Position { x: 1.0, y: 0.0, z: 0.0 }));
    app.update();
    let entry = result(&mut app);
    assert_eq!(entry.seq, 2);
    assert_eq!(entry.movement.pos.x, 1.0);
}

#[test]
fn a_report_replaces_the_retained_vertical_velocity_whole() {
    let (mut app, _) = movement_app(MapLayout::default());
    let mut falling = report(1, Position { y: 2.0, ..default() });
    falling.movement.vertical_velocity = -8.0;
    deliver(&mut app, falling);
    app.update();
    assert_eq!(result(&mut app).movement.vertical_velocity, -8.0);
    deliver(&mut app, report(2, Position::default()));
    app.update();
    assert_eq!(result(&mut app).movement.vertical_velocity, 0.0);
}

#[test]
fn reports_naming_an_unknown_carrier_are_rejected() {
    let (mut app, entity) = movement_app(MapLayout::default());
    deliver(&mut app, report(1, Position { x: 1.0, ..default() }));
    let mut stray = report(2, Position { x: 50.0, ..default() });
    stray.movement.carrier = CarrierId(1);
    deliver(&mut app, stray);
    app.update();
    let entry = result(&mut app);
    assert_eq!(entry.seq, 1);
    assert_eq!(entry.movement.pos.x, 1.0);
    assert_eq!(app.world().get::<Position>(entity).expect("position missing").x, 1.0);
    deliver(&mut app, report(2, Position { x: 2.0, ..default() }));
    app.update();
    assert_eq!(result(&mut app).movement.pos.x, 2.0);
}

#[test]
fn reports_while_dead_are_dropped_and_keep_the_sequence_cutoff() {
    let (mut app, entity) = movement_app(MapLayout::default());
    deliver(&mut app, report(5, Position { x: 1.0, ..default() }));
    app.update();
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&ID)
        .expect("player missing")
        .begin_respawn(1.0);
    deliver(&mut app, report(6, Position { x: 9.0, ..default() }));
    app.update();
    let info = player_info(&app);
    assert_eq!(info.session.last_move_seq, 5);
    assert_eq!(info.life.movement.pos.x, 0.0);
    assert_eq!(app.world().get::<Position>(entity).expect("position missing").x, 1.0);
    let mut fresh = report(6, Position { x: 3.0, ..default() });
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&ID)
        .expect("player missing")
        .finish_respawn(entity);
    fresh.generation = PlayerGeneration(1);
    deliver(&mut app, fresh);
    app.update();
    assert_eq!(player_info(&app).session.last_move_seq, 6);
    assert_eq!(result(&mut app).movement.pos.x, 3.0);
}

#[test]
fn reported_support_persists_without_fresh_reports() {
    let (mut app, _) = movement_app(MapLayout::default());
    app.world_mut().resource_mut::<MapSettings>().movement.gravity = 0.0;
    let mut message = report(1, Position::default());
    message.movement.support = CharacterSupport::Ground;
    deliver(&mut app, message);
    app.update();
    assert_eq!(player_info(&app).life.movement.support, CharacterSupport::Ground);
    assert_eq!(result(&mut app).movement.support, CharacterSupport::Ground);
    app.update();
    assert_eq!(player_info(&app).life.movement.support, CharacterSupport::Ground);
    assert_eq!(result(&mut app).movement.support, CharacterSupport::Ground);
}

#[test]
fn delayed_jittered_and_lost_reports_stay_accepted_during_regular_travel() {
    let (mut app, _) = movement_app(MapLayout::default());
    {
        let mut settings = app.world_mut().resource_mut::<MapSettings>();
        settings.movement.gravity = 0.0;
        settings.movement.player.walk_speed = 3.0;
    }
    let mut queue = Vec::new();
    let mut last_seq = 0;
    for tick in 1u32..=240 {
        if tick <= 200 && tick % 7 != 0 && !(80..100).contains(&tick) {
            let mut message = report(
                tick,
                Position {
                    x: tick as f32 * 0.1,
                    y: 0.0,
                    z: 0.0,
                },
            );
            message.movement.move_intent = PlayerMoveIntent::Walking {
                direction: std::f32::consts::FRAC_PI_2,
            };
            queue.push((tick + 6 + (tick * 7 % 9), message));
        }
        let mut delivered = Vec::new();
        queue.retain(|(deadline, message)| {
            if *deadline <= tick {
                delivered.push(message.clone());
                false
            } else {
                true
            }
        });
        delivered.reverse();
        for message in delivered {
            deliver(&mut app, message);
        }
        app.update();
        let entry = result(&mut app);
        assert!((entry.movement.pos.x - entry.seq as f32 * 0.1).abs() < 1e-5);
        last_seq = entry.seq;
    }
    assert_eq!(last_seq, 200);
}
