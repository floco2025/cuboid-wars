use super::*;
use crate::test_fixtures;
use common::protocol::{CarrierId, Eraser, Lane, MapLayout, PlayerGeneration};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

fn app(layout: MapLayout) -> (App, Entity, UnboundedReceiver<ClientMessage>) {
    let mut app = App::new();
    let (tx, rx) = unbounded_channel();
    app.insert_resource(test_fixtures::gameplay_config())
        .insert_resource(CollisionWorld::from_map_layout(&layout))
        .insert_resource(Carriers::from_layout(&layout))
        .insert_resource(NetworkConfig {
            update_hz: 30,
            ..default()
        })
        .init_resource::<LocalPlayerInfo>()
        .insert_resource(ClientToServerChannel::new(tx))
        .add_systems(Update, report_move_outcomes_system);
    let entity = app
        .world_mut()
        .spawn((
            LocalPlayerMarker,
            Position::default(),
            LocalMovementStep {
                start: Position::default(),
                crushed: false,
                impact_speed: 0.0,
                carrier: CarrierId::WORLD,
                support: CharacterSupport::Airborne,
            },
        ))
        .id();
    (app, entity, rx)
}

fn messages(rx: &mut UnboundedReceiver<ClientMessage>) -> Vec<MoveOutcome> {
    std::iter::from_fn(|| rx.try_recv().ok())
        .map(|command| {
            let message @ ClientMessage::MoveOutcome(_) = command else {
                panic!("unexpected command")
            };
            assert_eq!(message.lane(), Lane::Reliable);
            let ClientMessage::MoveOutcome(event) = message else {
                unreachable!()
            };
            event.event
        })
        .collect()
}

#[test]
fn landing_reports_the_motor_impact_speed_and_portal_crossings_suppress_impacts() {
    for crossing in [false, true] {
        let (mut app, entity, mut rx) = app(MapLayout::default());
        app.world_mut()
            .get_mut::<LocalMovementStep>(entity)
            .expect("step missing")
            .impact_speed = 20.0;
        if crossing {
            app.world_mut()
                .resource_mut::<LocalPlayerInfo>()
                .reports
                .begin_crossing(Position::default());
        }
        app.update();
        let events = messages(&mut rx);
        if crossing {
            assert!(events.is_empty());
        } else {
            assert!(matches!(
                events.as_slice(),
                [MoveOutcome::Landed { impact_speed: 20.0, .. }]
            ));
        }
        app.world_mut()
            .get_mut::<LocalMovementStep>(entity)
            .expect("step missing")
            .impact_speed = 0.0;
        app.update();
        assert!(messages(&mut rx).is_empty());
    }
}

#[test]
fn eraser_sweeps_cover_travel_and_both_portal_ends_without_sweeping_the_gap() {
    for (eraser_x, crossing, expected) in [
        (1.0, false, true),
        (1.0, true, true),
        (5.0, true, false),
        (100.0, true, true),
    ] {
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
        let (mut app, entity, mut rx) = app(layout);
        if crossing {
            app.world_mut()
                .resource_mut::<LocalPlayerInfo>()
                .reports
                .begin_crossing(Position { x: 2.0, ..default() });
        }
        app.world_mut().get_mut::<Position>(entity).expect("position missing").x = if crossing { 100.0 } else { 2.0 };
        app.update();
        assert_eq!(
            messages(&mut rx)
                .iter()
                .any(|event| matches!(event, MoveOutcome::EraseEquipment)),
            expected
        );
    }
}

#[test]
fn a_dead_local_player_sends_no_movement_outcome() {
    let (mut app, entity, mut rx) = app(MapLayout::default());
    let mut step = app
        .world_mut()
        .get_mut::<LocalMovementStep>(entity)
        .expect("step missing");
    step.impact_speed = 20.0;
    step.crushed = true;
    app.world_mut().get_mut::<Position>(entity).expect("position missing").y = CHARACTER_FALL_DEATH_Y - 1.0;
    app.world_mut().resource_mut::<LocalPlayerInfo>().is_dead = true;
    app.update();
    assert!(messages(&mut rx).is_empty());
}

#[test]
fn standing_in_an_eraser_repeats_at_the_movement_cadence_and_re_entry_reports_at_once() {
    let layout = MapLayout {
        erasers: vec![Eraser {
            x1: 0.0,
            x2: 0.0,
            z1: -5.0,
            z2: 5.0,
            y: -1.0,
            height: 5.0,
            width: 0.5,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    let (mut app, entity, mut rx) = app(layout);
    app.world_mut().insert_resource(NetworkConfig {
        server_hz: 30,
        update_hz: 10,
        snapshot_hz: 4,
    });
    let erased = |rx: &mut UnboundedReceiver<ClientMessage>| {
        messages(rx)
            .iter()
            .any(|event| matches!(event, MoveOutcome::EraseEquipment))
    };
    let mut reports = 0;
    for _ in 0..6 {
        app.update();
        reports += usize::from(erased(&mut rx));
    }
    assert_eq!(reports, 2, "entry plus one cadence tick over six ticks at 10 Hz");
    let place = |app: &mut App, x: f32| {
        app.world_mut().get_mut::<Position>(entity).expect("position missing").x = x;
        app.world_mut()
            .get_mut::<LocalMovementStep>(entity)
            .expect("step missing")
            .start
            .x = x;
    };
    place(&mut app, 50.0);
    app.update();
    assert!(!erased(&mut rx));
    place(&mut app, 0.0);
    app.update();
    assert!(erased(&mut rx), "re-entry is reported at once");
}

#[test]
fn crushing_reports_while_contact_lasts_and_void_reports_once_per_body() {
    let (mut app, entity, mut rx) = app(MapLayout::default());
    app.world_mut()
        .get_mut::<LocalMovementStep>(entity)
        .expect("step missing")
        .crushed = true;
    app.update();
    assert!(matches!(messages(&mut rx).as_slice(), [MoveOutcome::Crushed { .. }]));
    app.update();
    assert!(matches!(messages(&mut rx).as_slice(), [MoveOutcome::Crushed { .. }]));
    app.world_mut()
        .get_mut::<LocalMovementStep>(entity)
        .expect("step missing")
        .crushed = false;
    app.world_mut().get_mut::<Position>(entity).expect("position missing").y = CHARACTER_FALL_DEATH_Y - 1.0;
    app.update();
    assert!(matches!(messages(&mut rx).as_slice(), [MoveOutcome::FellOutOfWorld]));
    app.update();
    assert!(messages(&mut rx).is_empty());
    app.world_mut()
        .resource_mut::<LocalPlayerInfo>()
        .reports
        .begin_body(PlayerGeneration(1));
    app.update();
    assert!(matches!(messages(&mut rx).as_slice(), [MoveOutcome::FellOutOfWorld]));
}
