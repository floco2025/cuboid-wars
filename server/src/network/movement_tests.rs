use super::handlers::handle_move_message;
use crate::{
    actors::ActorMap,
    characters::characters_movement_system,
    config::ServerGameplayConfig,
    players::{
        EraserContacts, PlayerInfo, PlayerMap, PlayerStateQuery, apply_pending_player_inputs_system,
        finish_player_movement_system,
    },
    portals::players_portal_traversal_system,
};
use bevy::{ecs::system::SystemState, prelude::*};
use common::{
    constants::TICK_DURATION,
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterSupport, CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity, PortalSet,
    },
    protocol::*,
};
use tokio::sync::mpsc::unbounded_channel;

const ID: PlayerId = PlayerId(1);

fn app_without_portal_traversal(layout: MapLayout) -> (App, Entity) {
    let config = ServerGameplayConfig::load_default().expect("gameplay config missing");
    let mut app = App::new();
    let mut time = Time::<()>::default();
    time.advance_by(TICK_DURATION);
    app.insert_resource(time)
        .insert_resource(config.gameplay_config())
        .insert_resource(config.maps["hotel"].settings.clone())
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
            PlayerMoveIntent::Idle,
            FaceYaw(0.0),
            CharacterVerticalVelocity(0.0),
            AirborneMomentum::default(),
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
        input: PlayerInput {
            move_intent: PlayerMoveIntent::Idle,
            face_yaw: 0.0,
        },
        hops: 0,
        movement: PlayerMovementState::new(pos, PlayerMoveIntent::Idle, 0.0, 0.0),
        result_hops: 0,
    }
}

fn deliver(app: &mut App, message: CMove) {
    handle_move_message(ID, message, &mut app.world_mut().resource_mut::<PlayerMap>());
}

fn result(app: &mut App) -> PlayerMove {
    let mut state = SystemState::<(
        PlayerStateQuery,
        Query<
            (
                &CharacterVerticalVelocity,
                Option<&AirborneMomentum>,
                Option<&KnockbackVelocity>,
            ),
            With<PlayerMarker>,
        >,
    )>::new(app.world_mut());
    let (positions, motions) = state.get(app.world()).expect("movement collection state invalid");
    super::broadcast::collect_player_moves(app.world().resource::<PlayerMap>(), &positions, &motions)
        .into_iter()
        .find(|entry| entry.id == ID)
        .expect("movement missing")
}

#[test]
fn accepts_client_state_through_a_wall_and_with_a_different_crossing_count() {
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
    let (mut app, entity) = app_without_portal_traversal(layout);
    let mut message = report(1, Position { x: 2.0, y: 1.0, z: 0.0 });
    message.movement = PlayerMovementState::new(
        message.movement.pos,
        PlayerMoveIntent::Running { direction: 2.0 },
        7.0,
        2.0,
    )
    .with_momentum(Vec3::new(-3.0, 0.0, 2.0), Vec3::X);
    message.result_hops = 4;
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
    assert_eq!(result(&mut app).hops, 4);
    assert_eq!(result(&mut app).movement.pos, expected.pos);
}

#[test]
fn distance_rejection_keeps_prediction_and_later_ticks_do_not_repeat_the_sequence() {
    let (mut app, entity) = app_without_portal_traversal(MapLayout::default());
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
    let (mut app, entity) = app_without_portal_traversal(MapLayout::default());
    let mut stale = report(1, Position { x: 3.0, y: 0.0, z: 0.0 });
    stale.input.move_intent = PlayerMoveIntent::Running { direction: 1.0 };
    deliver(&mut app, stale.clone());
    deliver(&mut app, report(3, Position { x: 1.0, y: 0.0, z: 0.0 }));
    deliver(&mut app, stale);
    app.update();
    assert_eq!(result(&mut app).move_seq, Some(3));
    assert_eq!(result(&mut app).movement.pos.x, 1.0);
    assert_eq!(
        *app.world().get::<PlayerMoveIntent>(entity).expect("intent missing"),
        PlayerMoveIntent::Idle
    );
}

#[test]
fn non_finite_reports_do_not_advance_sequence_or_replace_fresh_state() {
    let (mut app, _) = app_without_portal_traversal(MapLayout::default());
    deliver(&mut app, report(1, Position::default()));
    app.update();
    let mut invalid = report(101, Position::default());
    invalid.movement.knockback[0] = f32::NAN;
    deliver(&mut app, invalid);
    deliver(&mut app, report(2, Position { x: 1.0, y: 0.0, z: 0.0 }));
    app.update();
    assert_eq!(result(&mut app).move_seq, Some(2));
    assert_eq!(result(&mut app).movement.pos.x, 1.0);
}

#[test]
fn accepted_landing_refreshes_support_and_stops_server_fall_velocity() {
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
    let (mut app, entity) = app_without_portal_traversal(layout);
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
    assert_eq!(
        app.world()
            .resource::<PlayerMap>()
            .get(&ID)
            .expect("player missing")
            .life
            .fall_state
            .support(),
        CharacterSupport::Ground
    );
}

#[test]
fn accepted_same_side_motion_sweeps_erasers_but_portal_discontinuity_does_not() {
    for (hops, expected) in [(0, true), (1, false)] {
        let layout = MapLayout {
            erasers: vec![Eraser {
                x1: 1.0,
                x2: 1.0,
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
        let (mut app, _) = app_without_portal_traversal(layout);
        let mut message = report(1, Position { x: 2.0, y: 0.0, z: 0.0 });
        message.result_hops = hops;
        deliver(&mut app, message);
        app.update();
        assert_eq!(!app.world().resource::<EraserContacts>().swept.is_empty(), expected);
    }
}

#[test]
fn mirrored_portal_crossing_is_accepted_once_and_continues_away_from_exit() {
    let (mut app, entity) = app_without_portal_traversal(MapLayout::default());
    app.add_systems(
        Update,
        players_portal_traversal_system
            .after(characters_movement_system)
            .before(finish_player_movement_system),
    );
    let portals = [PortalEnd::A, PortalEnd::B]
        .into_iter()
        .enumerate()
        .map(|(i, end)| Portal {
            pair: PortalPairId(1),
            end,
            pos: Position {
                x: i as f32 * 10.0,
                y: 1.6,
                z: 0.0,
            },
            nx: 0.0,
            ny: 0.0,
            nz: 1.0,
            yaw: 0.0,
            carrier: CarrierId::WORLD,
        })
        .collect::<Vec<_>>();
    let set = PortalSet::rebuild(&portals, app.world().resource::<CollisionWorld>(), &Carriers::default());
    let start = Position {
        x: 0.0,
        y: 0.7,
        z: 0.05,
    };
    app.world_mut()
        .get_mut::<Position>(entity)
        .expect("position missing")
        .clone_from(&start);
    let intent = PlayerMoveIntent::Running {
        direction: std::f32::consts::PI,
    };
    *app.world_mut()
        .get_mut::<PlayerMoveIntent>(entity)
        .expect("intent missing") = intent;
    app.insert_resource(set);
    app.update();
    let info = app.world().resource::<PlayerMap>().get(&ID).expect("player missing");
    assert_eq!(info.session.hops, 1);
    let exit = *app.world().get::<Position>(entity).expect("position missing");
    assert!((exit.x - 10.0).abs() < 0.01);
    let mut message = report(1, exit);
    message.hops = 1;
    message.result_hops = 1;
    message.input.move_intent = PlayerMoveIntent::Running { direction: 0.0 };
    message.movement.move_intent = message.input.move_intent;
    deliver(&mut app, message);
    app.update();
    assert_eq!(result(&mut app).movement.pos, exit);
    app.update();
    assert_eq!(
        app.world()
            .resource::<PlayerMap>()
            .get(&ID)
            .expect("player missing")
            .session
            .hops,
        1
    );
    assert!(app.world().get::<Position>(entity).expect("position missing").z > exit.z);
}

#[test]
fn delayed_jittered_and_lost_reports_stay_accepted_during_regular_travel() {
    let (mut app, _) = app_without_portal_traversal(MapLayout::default());
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
            message.input.move_intent = PlayerMoveIntent::Walking {
                direction: std::f32::consts::FRAC_PI_2,
            };
            message.movement.move_intent = message.input.move_intent;
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
