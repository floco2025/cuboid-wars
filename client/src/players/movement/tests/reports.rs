use super::*;
use bevy::prelude::*;
use common::{
    physics::CharacterSupport,
    protocol::{Carrier, MapLayout, PlayerGeneration, SwitchState},
};

fn state(pos: Position, support: CharacterSupport) -> PlayerMovementState {
    PlayerMovementState {
        support,
        ..PlayerMovementState::new(pos, Default::default(), 0.0, 0.0)
    }
}

fn slider(from: Position, to: Position) -> Carriers {
    Carriers::from_layout(&MapLayout {
        carriers: vec![Carrier {
            motion: Default::default(),
            initially_on: true,
            parent: CarrierId::WORLD,
            level: 0,
            levels: 1,
            from,
            to,
            travel_ticks: 30,
            pause_ticks: 10,
            phase_ticks: 0,
            switch: None,
        }],
        ..default()
    })
}

#[test]
fn grounded_rider_reports_local_position_and_takeoff_immediately_returns_to_world_space() {
    let mut carriers = slider(
        Position {
            x: 20.0,
            y: 0.0,
            z: 0.0,
        },
        Position {
            x: 30.0,
            y: 5.0,
            z: 0.0,
        },
    );
    let network = NetworkConfig {
        update_hz: 10,
        ..default()
    };
    let mut reports = LocalMovementReports::default();
    let local = Position {
        x: 1.0,
        y: 0.0,
        z: -1.0,
    };
    let mut sent = 0;
    for tick in 1..=41 {
        carriers.advance(tick, &SwitchState::default());
        let pos = carriers.pose(CarrierId(1)).transform_position(&local);
        if let Some(report) =
            reports.movement_report(&network, state(pos, CharacterSupport::Ground), CarrierId(1), &carriers)
        {
            assert_eq!(report.movement.carrier, CarrierId(1));
            assert!((Vec3::from(report.movement.pos) - Vec3::from(local)).length() < 1e-5);
            sent += 1;
        }
    }
    assert_eq!(sent, 14);
    let takeoff = Position {
        x: 31.0,
        y: 6.0,
        z: -1.0,
    };
    let report = reports
        .movement_report(
            &network,
            state(takeoff, CharacterSupport::Airborne),
            CarrierId::WORLD,
            &carriers,
        )
        .expect("takeoff report missing");
    assert_eq!(report.movement.carrier, CarrierId::WORLD);
    assert_eq!(report.movement.pos, takeoff);
}

#[test]
fn boarding_a_carrier_reports_immediately() {
    let carriers = slider(Position::default(), Position { x: 4.0, y: 0.0, z: 0.0 });
    let network = NetworkConfig {
        update_hz: 1,
        ..default()
    };
    let mut reports = LocalMovementReports::default();
    let standing = state(Position::default(), CharacterSupport::Ground);
    assert!(
        reports
            .movement_report(&network, standing, CarrierId::WORLD, &carriers)
            .is_some(),
        "the first report is due at once"
    );
    assert!(
        reports
            .movement_report(&network, standing, CarrierId::WORLD, &carriers)
            .is_none(),
        "the cadence holds off the next report"
    );
    let report = reports
        .movement_report(&network, standing, CarrierId(1), &carriers)
        .expect("boarding report missing");
    assert_eq!(report.movement.carrier, CarrierId(1));
}

#[test]
fn new_body_clears_crossings_and_reports_immediately_without_resetting_sequence() {
    let network = NetworkConfig {
        update_hz: 1,
        ..default()
    };
    let mut reports = LocalMovementReports {
        seq: u32::MAX - 1,
        ..default()
    };
    assert!(reports.report_due(&network, CarrierId::WORLD));
    reports.begin_crossing(Position::default());
    reports.begin_body(PlayerGeneration(1));
    assert_eq!(reports.portal_crossing, 0);
    assert!(reports.crossing_entrance.is_none());
    assert!(reports.report_due(&network, CarrierId::WORLD));
    assert_eq!(reports.seq, 0);
    assert!(!reports.report_due(&network, CarrierId::WORLD));
    assert_eq!(reports.seq, 1);
}

#[test]
fn a_crossing_reports_at_once_in_world_space_and_later_reports_repeat_its_count() {
    let carriers = Carriers::default();
    let network = NetworkConfig {
        update_hz: 5,
        ..default()
    };
    let mut reports = LocalMovementReports::default();
    let airborne = state(Position { x: 10.0, ..default() }, CharacterSupport::Airborne);
    let first = reports
        .movement_report(&network, airborne, CarrierId::WORLD, &carriers)
        .expect("the first report is due");
    assert_eq!((first.seq, first.portal_crossing), (1, 0));
    assert!(
        reports
            .movement_report(&network, airborne, CarrierId::WORLD, &carriers)
            .is_none()
    );
    reports.begin_crossing(Position::default());
    let crossing = reports
        .movement_report(&network, airborne, CarrierId(1), &carriers)
        .expect("a crossing reports at once");
    assert_eq!((crossing.seq, crossing.portal_crossing), (3, 1));
    assert_eq!(
        crossing.movement.carrier,
        CarrierId::WORLD,
        "a crossing lands in world space"
    );
    assert!(
        reports
            .movement_report(&network, airborne, CarrierId::WORLD, &carriers)
            .is_none()
    );
    reports.begin_crossing(Position::default());
    let again = reports
        .movement_report(&network, airborne, CarrierId::WORLD, &carriers)
        .expect("a second crossing reports at once");
    assert_eq!((again.seq, again.portal_crossing), (5, 2));
}
