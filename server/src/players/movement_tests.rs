use crate::{
    actors::ActorMap,
    characters::{MovementStart, characters_movement_system},
    config::ServerGameplayConfig,
    network::{ServerToClient, collect_player_moves},
    players::{
        EraserContacts, PlayerInfo, PlayerMap, PlayerMotionQuery, PlayerMovementReport, PlayerStateQuery,
        apply_pending_player_inputs_system, finish_player_movement_system, handle_portal_recovery_message,
        queue_player_movement,
    },
};
use bevy::{ecs::system::SystemState, prelude::*};
use common::{
    config::GameplayConfig,
    constants::TICK_DURATION,
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterSupport, CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity, PortalSet,
    },
    protocol::*,
};
use tokio::sync::mpsc::unbounded_channel;

const ID: PlayerId = PlayerId(1);

fn movement_app(layout: MapLayout) -> (App, Entity) {
    let config = ServerGameplayConfig::load_default().expect("gameplay config missing");
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
        .insert_resource(CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default()))
        .insert_resource(Carriers::from_layout(&layout))
        .init_resource::<PortalSet>()
        .init_resource::<PlayerMap>()
        .init_resource::<ActorMap>()
        .init_resource::<PlateState>()
        .init_resource::<EraserContacts>()
        .init_resource::<ServerTick>()
        .add_systems(
            Update,
            (
                server_tick_advance_system,
                apply_pending_player_inputs_system,
                characters_movement_system,
                finish_player_movement_system,
            )
                .chain(),
        );
    let entity = app
        .world_mut()
        .spawn((
            ID,
            PlayerMarker,
            Position::default(),
            MovementStart(Position::default()),
            PlayerMoveIntent::Idle,
            FaceYaw(0.0),
            CharacterVerticalVelocity(0.0),
            AirborneMomentum::default(),
            KnockbackVelocity::default(),
            Health(100.0),
        ))
        .id();
    let (sender, _) = unbounded_channel();
    let mut info = PlayerInfo::new(entity, sender);
    info.connection.logged_in = true;
    app.world_mut().resource_mut::<PlayerMap>().insert(ID, info);
    (app, entity)
}

fn report(seq: u32, pos: Position) -> CMove {
    CMove {
        seq,
        movement: PlayerMovementState::new(pos, PlayerMoveIntent::Idle, 0.0, 0.0),
    }
}

fn deliver(app: &mut App, message: CMove) {
    queue_player_movement(
        ID,
        PlayerMovementReport::Move(message),
        &mut app.world_mut().resource_mut::<PlayerMap>(),
    );
}

fn cross(app: &mut App, seq: u32, entrance_x: f32, exit_x: f32) {
    queue_player_movement(
        ID,
        PlayerMovementReport::PortalCross(CPortalCross {
            seq,
            entrance: report(
                seq,
                Position {
                    x: entrance_x,
                    ..default()
                },
            )
            .movement,
            movement: report(seq, Position { x: exit_x, ..default() }).movement,
        }),
        &mut app.world_mut().resource_mut::<PlayerMap>(),
    );
}

fn player_info(app: &App) -> &PlayerInfo {
    app.world().resource::<PlayerMap>().get(&ID).expect("player missing")
}

#[test]
fn crossing_boundaries_survive_coalesced_moves_and_consecutive_crossings() {
    let (mut app, _) = movement_app(MapLayout::default());
    app.world_mut().resource_mut::<MapSettings>().movement.gravity = 0.0;
    deliver(&mut app, report(1, Position::default()));
    cross(&mut app, 2, 0.0, 100.0);
    deliver(&mut app, report(3, Position { x: 101.0, ..default() }));
    deliver(&mut app, report(4, Position { x: 102.0, ..default() }));
    cross(&mut app, 5, 102.0, -100.0);
    deliver(&mut app, report(6, Position { x: -99.0, ..default() }));
    deliver(&mut app, report(1, Position::default()));
    for (seq, x) in [(1, 0.0), (2, 100.0), (4, 102.0), (5, -100.0), (6, -99.0)] {
        app.update();
        let entry = result(&mut app);
        assert_eq!(entry.move_seq, Some(seq));
        assert_eq!(entry.movement.pos.x, x);
    }
}

#[test]
fn rejection_discards_dependent_moves_until_recovery_and_filters_delayed_packets() {
    let (mut app, _) = movement_app(MapLayout::default());
    app.world_mut().resource_mut::<MapSettings>().movement.gravity = 0.0;
    cross(&mut app, 1, 5.0, 100.0);
    cross(&mut app, 2, 100.0, 200.0);
    deliver(&mut app, report(3, Position { x: 201.0, ..default() }));
    app.update();
    assert_eq!(result(&mut app).movement.pos.x, 0.0);
    assert!(player_info(&app).life.portal_recovery_pending);
    deliver(&mut app, report(4, Position { x: 1.0, ..default() }));
    cross(&mut app, 5, 0.0, 300.0);
    // This fresh datagram overtakes recovery; dropping it must not advance the cutoff.
    deliver(&mut app, report(8, Position { x: 1.0, ..default() }));
    app.update();
    let entry = result(&mut app);
    assert_eq!(entry.move_seq, None);
    assert_eq!(entry.movement.pos.x, 0.0);
    handle_portal_recovery_message(
        ID,
        CPortalRecovery { seq: 7 },
        &mut app.world_mut().resource_mut::<PlayerMap>(),
    );
    deliver(&mut app, report(6, Position { x: 2.0, ..default() }));
    app.update();
    assert_eq!(result(&mut app).move_seq, None);
    deliver(&mut app, report(9, Position { x: 1.0, ..default() }));
    app.update();
    let entry = result(&mut app);
    assert_eq!(entry.move_seq, Some(9));
    assert_eq!(entry.movement.pos.x, 1.0);
}

#[test]
fn crossing_recovery_and_full_exit_motion_survive_sequence_wrap() {
    let (mut app, _) = movement_app(MapLayout::default());
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&ID)
        .expect("player missing")
        .session
        .last_move_seq = u32::MAX - 1;
    cross(&mut app, u32::MAX, 5.0, 100.0);
    app.update();
    assert_eq!(result(&mut app).movement.pos.x, 0.0);
    handle_portal_recovery_message(
        ID,
        CPortalRecovery { seq: 0 },
        &mut app.world_mut().resource_mut::<PlayerMap>(),
    );
    cross(&mut app, u32::MAX, 0.0, 200.0);
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
    queue_player_movement(
        ID,
        PlayerMovementReport::PortalCross(CPortalCross {
            seq: 1,
            entrance: result(&mut app).movement,
            movement,
        }),
        &mut app.world_mut().resource_mut::<PlayerMap>(),
    );
    app.update();
    let entry = result(&mut app);
    assert_eq!(entry.move_seq, Some(1));
    assert_eq!(entry.movement.pos, movement.pos);
    assert_eq!(entry.movement.move_intent, movement.move_intent);
    assert_eq!(entry.movement.face_yaw, movement.face_yaw);
    assert_eq!(entry.movement.vertical_velocity, movement.vertical_velocity);
    assert_eq!(entry.movement.airborne_momentum, movement.airborne_momentum);
    assert_eq!(entry.movement.knockback, movement.knockback);
    assert_eq!(entry.movement.support, movement.support);
}

#[test]
fn accepted_crossings_broadcast_to_observers_and_rejections_only_to_the_owner() {
    for accepted in [true, false] {
        let (mut app, _) = movement_app(MapLayout::default());
        let (owner_sender, mut owner) = unbounded_channel();
        let (observer_sender, mut observer) = unbounded_channel();
        {
            let mut players = app.world_mut().resource_mut::<PlayerMap>();
            players.get_mut(&ID).expect("player missing").connection.channel = owner_sender;
            let mut info = PlayerInfo::new(Entity::PLACEHOLDER, observer_sender);
            info.connection.logged_in = true;
            players.insert(PlayerId(2), info);
        }
        cross(&mut app, 1, if accepted { 0.0 } else { 5.0 }, 100.0);
        app.update();
        let ServerToClient::Send(ServerMessage::PortalCrossed(event)) =
            owner.try_recv().expect("crossing response missing")
        else {
            panic!("unexpected response")
        };
        assert_eq!(event.accepted, accepted);
        assert_eq!(event.seq, 1);
        assert_eq!(event.movement.pos.x, if accepted { 100.0 } else { 0.0 });
        assert_eq!(observer.try_recv().is_ok(), accepted);
    }
}

#[test]
fn eraser_sweeps_cover_the_entrance_motion_without_crossing_the_portal_gap() {
    for (eraser_x, crosses, expected) in [(1.0, false, true), (1.0, true, true), (5.0, true, false)] {
        let layout = MapLayout {
            erasers: vec![Eraser {
                x1: eraser_x,
                x2: eraser_x,
                z1: -5.0,
                z2: 5.0,
                y: -1.0,
                height: 5.0,
                width: 0.1,
                level: 0,
                carrier: CarrierId::WORLD,
            }],
            ..default()
        };
        let (mut app, _) = movement_app(layout);
        if crosses {
            cross(&mut app, 1, 2.0, 100.0);
        } else {
            deliver(&mut app, report(1, Position { x: 2.0, ..default() }));
        }
        app.update();
        assert_eq!(!app.world().resource::<EraserContacts>().swept.is_empty(), expected);
    }
}

fn result(app: &mut App) -> PlayerMove {
    let mut state = SystemState::<(PlayerStateQuery, PlayerMotionQuery)>::new(app.world_mut());
    let (positions, motions) = state.get(app.world()).expect("movement collection state invalid");
    collect_player_moves(app.world().resource::<PlayerMap>(), &positions, &motions)
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
    assert_eq!(
        app.world()
            .get::<CharacterVerticalVelocity>(entity)
            .expect("vertical missing")
            .0,
        7.0
    );
    assert_eq!(
        app.world().get::<AirborneMomentum>(entity).expect("momentum missing").0,
        Vec3::new(-3.0, 0.0, 2.0)
    );
    assert_eq!(
        app.world()
            .get::<KnockbackVelocity>(entity)
            .expect("knockback missing")
            .0,
        Vec3::X
    );
    assert_eq!(app.world().get::<FaceYaw>(entity).expect("facing missing").0, 2.0);
    assert_eq!(result(&mut app).movement.pos, expected.pos);
}

#[test]
fn distance_rejection_keeps_prediction_and_later_ticks_do_not_repeat_the_sequence() {
    let (mut app, entity) = movement_app(MapLayout::default());
    deliver(&mut app, report(1, Position { x: 5.0, y: 0.0, z: 0.0 }));
    app.update();
    let rejected = result(&mut app);
    assert_eq!(rejected.movement.pos.x, 0.0);
    for _ in 0..10 {
        app.update();
    }
    let repeated = result(&mut app);
    assert_eq!(rejected.move_seq, Some(1));
    assert_eq!(repeated.move_seq, None);
    assert!(repeated.movement.pos.y < rejected.movement.pos.y);
    assert_eq!(
        *app.world().get::<Position>(entity).expect("position missing"),
        repeated.movement.pos
    );
}

#[test]
fn only_newest_report_steers_and_is_processed_even_when_packets_arrive_together() {
    let (mut app, entity) = movement_app(MapLayout::default());
    let mut stale = report(1, Position { x: 3.0, y: 0.0, z: 0.0 });
    stale.movement.move_intent = PlayerMoveIntent::Running { direction: 1.0 };
    deliver(&mut app, stale.clone());
    deliver(&mut app, report(3, Position { x: 1.0, y: 0.0, z: 0.0 }));
    deliver(&mut app, stale);
    app.update();
    let entry = result(&mut app);
    assert_eq!(entry.move_seq, Some(3));
    assert_eq!(entry.movement.pos.x, 1.0);
    assert_eq!(
        *app.world().get::<PlayerMoveIntent>(entity).expect("intent missing"),
        PlayerMoveIntent::Idle
    );
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
    assert_eq!(entry.move_seq, Some(2));
    assert_eq!(entry.movement.pos.x, 1.0);
}

#[test]
fn accepted_landing_stops_server_fall_velocity() {
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -5.0,
            x2: 5.0,
            z1: -5.0,
            z2: 5.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    let (mut app, entity) = movement_app(layout);
    app.world_mut().get_mut::<Position>(entity).expect("position missing").y = 2.0;
    app.world_mut()
        .get_mut::<CharacterVerticalVelocity>(entity)
        .expect("vertical missing")
        .0 = -8.0;
    deliver(&mut app, report(1, Position::default()));
    app.update();
    assert_eq!(
        app.world()
            .get::<CharacterVerticalVelocity>(entity)
            .expect("vertical missing")
            .0,
        0.0
    );
}

// The server's own step in empty space finds no support; the accepted
// report's support stands until the next tick without one.
#[test]
fn accepted_reports_carry_their_support_and_the_step_takes_over_without_one() {
    let (mut app, _) = movement_app(MapLayout::default());
    app.world_mut().resource_mut::<MapSettings>().movement.gravity = 0.0;
    let mut message = report(1, Position::default());
    message.movement.support = CharacterSupport::Ground;
    deliver(&mut app, message);
    app.update();
    assert_eq!(player_info(&app).life.fall_state.support(), CharacterSupport::Ground);
    assert_eq!(result(&mut app).movement.support, CharacterSupport::Ground);
    app.update();
    assert_eq!(player_info(&app).life.fall_state.support(), CharacterSupport::Airborne);
    assert_eq!(result(&mut app).movement.support, CharacterSupport::Airborne);
}

// Crushing is the one thing re-derived at an accepted position: a body
// reported inside a carrier's slab is crushed, one inside a world slab is
// not, as in the step.
#[test]
fn accepted_position_inside_a_carrier_slab_is_crushed() {
    let slab = Floor {
        x1: -1.5,
        z1: -1.5,
        x2: 1.5,
        z2: 1.5,
        y: 0.0,
        thickness: 0.4,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    for (carrier, expected) in [(CarrierId(1), true), (CarrierId::WORLD, false)] {
        let layout = MapLayout {
            floors: vec![Floor { carrier, ..slab }],
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 0,
                from: Position::default(),
                to: Position::default(),
                travel_ticks: 1,
                pause_ticks: 0,
                phase_ticks: 0,
            }],
            ..default()
        };
        let (mut app, _) = movement_app(layout);
        app.world_mut().resource_mut::<MapSettings>().movement.gravity = 0.0;
        deliver(&mut app, report(1, Position { y: -0.2, ..default() }));
        app.update();
        assert_eq!(player_info(&app).life.fall_state.is_crushed(), expected, "{carrier:?}");
    }
}

#[test]
fn accepted_world_overlap_is_crushing_only_when_this_body_was_lifted() {
    for (carrier_x, expected_crushed) in [(0.0, true), (100.0, false)] {
        let slab = Floor {
            x1: -2.0,
            z1: -2.0,
            x2: 2.0,
            z2: 2.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        };
        let layout = MapLayout {
            floors: vec![
                slab,
                Floor {
                    carrier: CarrierId(1),
                    ..slab
                },
            ],
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 1,
                from: Position {
                    x: carrier_x,
                    ..default()
                },
                to: Position {
                    x: carrier_x,
                    y: 6.0,
                    z: 0.0,
                },
                travel_ticks: 60,
                pause_ticks: 0,
                phase_ticks: 0,
            }],
            ..default()
        };
        let (mut app, entity) = movement_app(layout.clone());
        let height = app
            .world()
            .resource::<GameplayConfig>()
            .player
            .physics()
            .movement_collider
            .height;
        let layout = MapLayout {
            floors: layout
                .floors
                .iter()
                .copied()
                .chain([Floor {
                    y: height + 0.02 + slab.thickness,
                    ..slab
                }])
                .collect(),
            ..layout
        };
        let mut carriers = Carriers::from_layout(&layout);
        carriers.advance(1);
        let mut collision = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
        collision.set_carrier_poses(&carriers);
        app.insert_resource(carriers).insert_resource(collision);
        let accepted = Position { y: 0.1, ..default() };
        deliver(&mut app, report(1, accepted));
        app.update();
        assert_eq!(
            *app.world().get::<Position>(entity).expect("position missing"),
            accepted
        );
        assert_eq!(
            player_info(&app).life.fall_state.is_crushed(),
            expected_crushed,
            "carrier x={carrier_x}"
        );
    }
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
        if let Some(seq) = entry.move_seq {
            assert!((entry.movement.pos.x - seq as f32 * 0.1).abs() < 1e-5);
            last_seq = seq;
        }
    }
    assert_eq!(last_seq, 200);
}
