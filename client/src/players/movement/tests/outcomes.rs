use super::*;
use crate::test_fixtures;
use common::protocol::{CarrierId, Eraser, MapLayout, PlayerGeneration};

struct Owner {
    collision: CollisionWorld,
    carriers: Carriers,
    network: NetworkConfig,
    physics: CharacterPhysicsConfig,
    pos: Position,
    step: LocalMovementStep,
    reports: LocalMovementReports,
}

impl Owner {
    fn new(layout: MapLayout) -> Self {
        Self {
            collision: CollisionWorld::from_map_layout(&layout),
            carriers: Carriers::from_layout(&layout),
            network: NetworkConfig {
                update_hz: 30,
                ..default()
            },
            physics: test_fixtures::gameplay_config().player.physics(),
            pos: Position::default(),
            step: LocalMovementStep::default(),
            reports: LocalMovementReports::default(),
        }
    }

    fn outcomes(&mut self) -> Vec<MoveOutcome> {
        collect_move_outcomes(
            &self.pos,
            &self.step,
            self.physics,
            &self.collision,
            &self.carriers,
            &mut self.reports,
            &self.network,
        )
    }

    fn erased(&mut self) -> bool {
        self.outcomes()
            .iter()
            .any(|event| matches!(event, MoveOutcome::EraseEquipment))
    }
}

fn eraser_at(x: f32, width: f32) -> MapLayout {
    MapLayout {
        erasers: vec![Eraser {
            x1: x,
            x2: x,
            z1: -5.0,
            z2: 5.0,
            y: -1.0,
            height: 5.0,
            width,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    }
}

#[test]
fn landing_reports_the_motor_impact_speed_and_portal_crossings_suppress_impacts() {
    for crossing in [false, true] {
        let mut owner = Owner::new(MapLayout::default());
        owner.step.result.impact_speed = 20.0;
        if crossing {
            owner.reports.begin_crossing(Position::default());
        }
        let events = owner.outcomes();
        if crossing {
            assert!(events.is_empty());
        } else {
            assert!(matches!(
                events.as_slice(),
                [MoveOutcome::Landed { impact_speed: 20.0, .. }]
            ));
        }
        owner.reports.clear_crossings();
        owner.step.result.impact_speed = 0.0;
        assert!(owner.outcomes().is_empty());
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
        let mut owner = Owner::new(eraser_at(eraser_x, 0.1));
        if crossing {
            owner.reports.begin_crossing(Position { x: 2.0, ..default() });
        }
        owner.pos.x = if crossing { 100.0 } else { 2.0 };
        assert_eq!(owner.erased(), expected, "eraser at {eraser_x}, crossing {crossing}");
    }
}

#[test]
fn standing_in_an_eraser_repeats_at_the_movement_cadence_and_re_entry_reports_at_once() {
    let mut owner = Owner::new(eraser_at(0.0, 0.5));
    owner.network = NetworkConfig {
        server_hz: 30,
        update_hz: 10,
        snapshot_hz: 4,
    };
    let reports = (0..6).filter(|_| owner.erased()).count();
    assert_eq!(reports, 2, "entry plus one cadence tick over six ticks at 10 Hz");
    let place = |owner: &mut Owner, x: f32| {
        owner.pos.x = x;
        owner.step.start.x = x;
    };
    place(&mut owner, 50.0);
    assert!(!owner.erased());
    place(&mut owner, 0.0);
    assert!(owner.erased(), "re-entry is reported at once");
}

#[test]
fn crushing_reports_while_contact_lasts_and_void_reports_once_per_body() {
    let mut owner = Owner::new(MapLayout::default());
    owner.step.result.crushed = true;
    assert!(matches!(owner.outcomes().as_slice(), [MoveOutcome::Crushed { .. }]));
    assert!(matches!(owner.outcomes().as_slice(), [MoveOutcome::Crushed { .. }]));
    owner.step.result.crushed = false;
    owner.pos.y = CHARACTER_FALL_DEATH_Y - 1.0;
    assert!(matches!(owner.outcomes().as_slice(), [MoveOutcome::FellOutOfWorld]));
    assert!(owner.outcomes().is_empty());
    owner.reports.begin_body(PlayerGeneration(1));
    assert!(matches!(owner.outcomes().as_slice(), [MoveOutcome::FellOutOfWorld]));
}
