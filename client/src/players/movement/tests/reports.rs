use super::*;
use crate::{characters::PreviousTickPosition, players::PlayerMap, portals::portal_transit_system, test_fixtures};
use common::{
    map::Carriers,
    physics::{CharacterSupport, CollisionWorld, PortalSet},
    protocol::{Carrier, CarrierId, MapLayout, PlateState, PlayerId, PlayerMarker, Portal, PortalEnd, PortalPairId},
};
use std::f32::consts::PI;
use tokio::sync::mpsc::unbounded_channel;

fn step(carrier: CarrierId, support: CharacterSupport) -> LocalMovementStep {
    LocalMovementStep {
        start: Position::default(),
        crushed: false,
        impact_speed: 0.0,
        carrier,
        support,
    }
}

#[test]
fn grounded_rider_reports_local_position_and_takeoff_immediately_returns_to_world_space() {
    let layout = MapLayout {
        carriers: vec![Carrier {
            switch_inverted: false,

            parent: CarrierId::WORLD,
            level: 0,
            levels: 1,
            from: Position {
                x: 20.0,
                y: 0.0,
                z: 0.0,
            },
            to: Position {
                x: 30.0,
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
    let mut app = App::new();
    let (sender, mut receiver) = unbounded_channel();
    app.insert_resource(Carriers::from_layout(&layout))
        .insert_resource(NetworkConfig {
            update_hz: 10,
            ..default()
        })
        .insert_resource(ClientToServerChannel::new(sender))
        .init_resource::<LocalPlayerInfo>()
        .add_systems(Update, report_player_movement_system);
    let entity = app
        .world_mut()
        .spawn((
            LocalPlayerMarker,
            Position::default(),
            PlayerMoveIntent::Idle,
            FaceYaw(0.0),
            CharacterVerticalVelocity(0.0),
            AirborneMomentum::default(),
            KnockbackVelocity::default(),
            step(CarrierId(1), CharacterSupport::Ground),
        ))
        .id();
    let local = Position {
        x: 1.0,
        y: 0.0,
        z: -1.0,
    };
    let mut reports = 0;
    for tick in 1..=41 {
        let pos = app.world_mut().resource_scope(|_, mut carriers: Mut<Carriers>| {
            carriers.advance(tick, &PlateState::default());
            carriers.pose(CarrierId(1)).transform_position(&local)
        });
        app.world_mut().entity_mut(entity).insert(pos);
        app.update();
        if let Ok(ClientMessage::Move(report)) = receiver.try_recv() {
            assert_eq!(report.movement.carrier, CarrierId(1));
            assert!((Vec3::from(report.movement.pos) - Vec3::from(local)).length() < 1e-5);
            reports += 1;
        }
    }
    assert_eq!(reports, 14);
    let takeoff = Position {
        x: 31.0,
        y: 6.0,
        z: -1.0,
    };
    app.world_mut()
        .entity_mut(entity)
        .insert((takeoff, step(CarrierId::WORLD, CharacterSupport::Airborne)));
    app.update();
    let ClientMessage::Move(report) = receiver.try_recv().expect("takeoff report missing") else {
        panic!("expected movement report");
    };
    assert_eq!(report.movement.carrier, CarrierId::WORLD);
    assert_eq!(report.movement.pos, takeoff);
}

#[test]
fn boarding_a_carrier_reports_immediately() {
    let layout = MapLayout {
        carriers: vec![Carrier {
            switch_inverted: false,

            parent: CarrierId::WORLD,
            level: 0,
            levels: 1,
            from: Position::default(),
            to: Position { x: 4.0, y: 0.0, z: 0.0 },
            travel_ticks: 30,
            pause_ticks: 10,
            phase_ticks: 0,
            switch: None,
        }],
        ..default()
    };
    let mut app = App::new();
    let (sender, mut receiver) = unbounded_channel();
    app.insert_resource(Carriers::from_layout(&layout))
        .insert_resource(NetworkConfig {
            update_hz: 1,
            ..default()
        })
        .insert_resource(ClientToServerChannel::new(sender))
        .init_resource::<LocalPlayerInfo>()
        .add_systems(Update, report_player_movement_system);
    let entity = app
        .world_mut()
        .spawn((
            LocalPlayerMarker,
            Position::default(),
            PlayerMoveIntent::Idle,
            FaceYaw(0.0),
            CharacterVerticalVelocity(0.0),
            AirborneMomentum::default(),
            KnockbackVelocity::default(),
            step(CarrierId::WORLD, CharacterSupport::Ground),
        ))
        .id();
    app.update();
    assert!(receiver.try_recv().is_ok(), "the first report is due at once");
    app.update();
    assert!(receiver.try_recv().is_err(), "the cadence holds off the next report");
    app.world_mut()
        .entity_mut(entity)
        .insert(step(CarrierId(1), CharacterSupport::Ground));
    app.update();
    let ClientMessage::Move(report) = receiver.try_recv().expect("boarding report missing") else {
        panic!("expected movement report");
    };
    assert_eq!(report.movement.carrier, CarrierId(1));
}

#[test]
fn a_dead_local_player_sends_no_movement_report() {
    let mut app = App::new();
    let (sender, mut receiver) = unbounded_channel();
    app.init_resource::<Carriers>()
        .insert_resource(NetworkConfig::default())
        .insert_resource(ClientToServerChannel::new(sender))
        .init_resource::<LocalPlayerInfo>()
        .add_systems(Update, report_player_movement_system);
    app.world_mut().spawn((
        LocalPlayerMarker,
        Position::default(),
        PlayerMoveIntent::Idle,
        FaceYaw(0.0),
        CharacterVerticalVelocity(0.0),
        AirborneMomentum::default(),
        KnockbackVelocity::default(),
        step(CarrierId::WORLD, CharacterSupport::Ground),
    ));
    app.world_mut().resource_mut::<LocalPlayerInfo>().is_dead = true;
    app.update();
    assert!(receiver.try_recv().is_err());
}

#[test]
fn new_body_clears_crossings_and_reports_immediately_without_resetting_sequence() {
    let mut reports = LocalMovementReports {
        seq: u32::MAX - 1,
        ..default()
    };
    let mut cadence = UpdateCadence::new(1, 30);
    assert!(reports.report_due(&mut cadence, CarrierId::WORLD));
    reports.begin_crossing(Position::default());
    reports.begin_body(PlayerGeneration(1));
    assert_eq!(reports.portal_crossing, 0);
    assert!(reports.crossing_entrance.is_none());
    assert!(reports.report_due(&mut cadence, CarrierId::WORLD));
    assert_eq!(reports.seq, 0);
    assert!(!reports.report_due(&mut cadence, CarrierId::WORLD));
    assert_eq!(reports.seq, 1);
}

#[test]
fn crossings_send_immediately_and_repeat_the_boundary_in_later_reports() {
    let mut app = App::new();
    let (sender, mut receiver) = unbounded_channel();
    let collision = CollisionWorld::from_map_layout(&MapLayout::default());
    let portals: Vec<_> = [PortalEnd::A, PortalEnd::B]
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
        .collect();
    let portal_set = PortalSet::rebuild(&portals, &collision, &Carriers::default());
    app.insert_resource(portal_set)
        .insert_resource(collision)
        .init_resource::<Carriers>()
        .insert_resource(test_fixtures::gameplay_config())
        .insert_resource(test_fixtures::map_settings())
        .init_resource::<LocalPlayerInfo>()
        .init_resource::<PlayerMap>()
        .insert_resource(NetworkConfig {
            update_hz: 10,
            ..default()
        })
        .insert_resource(ClientToServerChannel::new(sender))
        .add_systems(Update, (portal_transit_system, report_player_movement_system).chain());
    let entrance = Position {
        x: 0.0,
        y: 0.7,
        z: -0.05,
    };
    let spawn = |world: &mut World, id| {
        world
            .spawn((
                PlayerId(id),
                PlayerMarker,
                entrance,
                PreviousTickPosition(Position { z: 0.15, ..entrance }),
                PlayerMoveIntent::Running { direction: PI },
                FaceYaw(PI),
                CharacterVerticalVelocity(-2.0),
                AirborneMomentum(Vec3::X * 4.0),
                KnockbackVelocity(Vec3::Z * 2.0),
                step(CarrierId::WORLD, CharacterSupport::Airborne),
            ))
            .id()
    };
    let local = spawn(app.world_mut(), 1);
    app.world_mut().entity_mut(local).insert(LocalPlayerMarker);
    let remote = spawn(app.world_mut(), 2);
    app.update();
    let ClientMessage::Move(crossing) = receiver.try_recv().expect("crossing missing") else {
        panic!("expected crossing event")
    };
    assert_eq!(crossing.seq, 1);
    assert_eq!(crossing.portal_crossing, 1);
    assert!((crossing.movement.pos.x - 10.0).abs() < 1e-5);
    assert!((crossing.movement.airborne_momentum[0] + 4.0).abs() < 1e-4);
    assert!((crossing.movement.knockback[2] + 2.0).abs() < 1e-4);
    assert_eq!(
        *app.world().get::<Position>(remote).expect("remote position missing"),
        entrance
    );
    assert!(receiver.try_recv().is_err());
    app.update();
    assert!(receiver.try_recv().is_err());
    app.world_mut()
        .resource_mut::<LocalPlayerInfo>()
        .reports
        .begin_crossing(entrance);
    app.update();
    assert!(matches!(
        receiver.try_recv(),
        Ok(ClientMessage::Move(CMove {
            seq: 3,
            portal_crossing: 2,
            ..
        }))
    ));
    app.update();
    assert!(matches!(
        receiver.try_recv(),
        Ok(ClientMessage::Move(CMove {
            seq: 4,
            portal_crossing: 2,
            ..
        }))
    ));
}
