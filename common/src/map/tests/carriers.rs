use super::*;
use crate::{
    protocol::{Position, SwitchId},
    test_geometry::LEVEL_HEIGHT,
};

fn slider() -> Carrier {
    Carrier {
        parent: CarrierId::WORLD,
        level: 0,
        levels: 0,
        from: Position::default(),
        to: Position { x: 4.0, y: 0.0, z: 0.0 },
        travel_ticks: 60,
        pause_ticks: 30,
        phase_ticks: 0,
        switch: None,
    }
}

fn lift() -> Carrier {
    Carrier {
        to: Position {
            x: 0.0,
            y: LEVEL_HEIGHT,
            z: 0.0,
        },
        pause_ticks: 0,
        levels: 1,
        ..slider()
    }
}

fn carriers_at(carriers: Vec<Carrier>, tick: u32) -> Carriers {
    let mut runtime = Carriers::from_layout(&MapLayout {
        carriers,
        ..Default::default()
    });
    runtime.advance(tick.wrapping_sub(1), &PlateState::default());
    runtime.advance(tick, &PlateState::default());
    runtime
}

const SLIDER: CarrierId = CarrierId(1);

#[test]
fn carrier_offset_is_from_at_phase_zero() {
    assert_eq!(carrier_offset_at(&slider(), 0), Vec3::ZERO);
    assert_eq!(carrier_offset_at(&slider(), 180), Vec3::ZERO);
}

#[test]
fn carrier_offset_holds_at_to_through_the_pause() {
    let carrier = slider();
    assert_eq!(carrier_offset_at(&carrier, 60), Vec3::new(4.0, 0.0, 0.0));
    assert_eq!(carrier_offset_at(&carrier, 89), Vec3::new(4.0, 0.0, 0.0));
    assert!(carrier_offset_at(&carrier, 91).x < 4.0);
}

#[test]
fn carrier_offset_returns_to_from_after_one_cycle() {
    let carrier = slider();
    let cycle = 2 * (carrier.travel_ticks + carrier.pause_ticks);
    assert_eq!(carrier_offset_at(&carrier, cycle), Vec3::ZERO);
    assert_eq!(carrier_offset_at(&carrier, cycle + 30), carrier_offset_at(&carrier, 30));
}

#[test]
fn world_pose_is_identity_and_displacement_zero() {
    let carriers = carriers_at(vec![slider()], 1);
    assert_eq!(carriers.pose(CarrierId::WORLD), CarrierPose::IDENTITY);
    assert_eq!(carriers.pose_between(CarrierId::WORLD, 0.5), CarrierPose::IDENTITY);
    assert_eq!(carriers.displacement(CarrierId::WORLD), Vec3::ZERO);
    assert!(Carriers::default().is_static());
    assert!(!carriers.is_static());
}

#[test]
fn displacement_is_the_carriers_tick_travel() {
    let carriers = carriers_at(vec![slider()], 1);
    assert!((carriers.displacement(SLIDER) - Vec3::new(4.0 / 60.0, 0.0, 0.0)).length() < 1e-5);
    assert_eq!(carriers.pose(SLIDER).translation, carrier_offset_at(&slider(), 1));
    let halfway = carriers.pose_between(SLIDER, 0.5).translation;
    assert!((halfway.x - 2.0 / 60.0).abs() < 1e-5, "halfway was {halfway}");
}

#[test]
fn a_child_carrier_rides_its_parent() {
    let child = Carrier {
        parent: SLIDER,
        ..lift()
    };
    let carriers = carriers_at(vec![slider(), child], 1);
    let expected = Vec3::new(4.0 / 60.0, LEVEL_HEIGHT / 60.0, 0.0);
    assert!(
        (carriers.pose(CarrierId(2)).translation - expected).length() < 1e-5,
        "child pose {:?}",
        carriers.pose(CarrierId(2))
    );
    assert!((carriers.displacement(CarrierId(2)) - expected).length() < 1e-5);
}

#[test]
fn a_sinking_lift_reports_its_drop() {
    let sinking = Carrier {
        phase_ticks: 60,
        ..lift()
    };
    let carriers = carriers_at(vec![sinking], 1);
    assert!((carriers.displacement(SLIDER).y + LEVEL_HEIGHT / 60.0).abs() < 1e-5);
}

fn switched() -> Carrier {
    Carrier {
        switch: Some(SwitchId(0)),
        ..slider()
    }
}

fn running_since(since_tick: u32, run_ticks: u32) -> PlateState {
    let mut state = PlateState::default();
    state.carrier_runs.push((
        SLIDER,
        CarrierRun {
            running: true,
            run_ticks,
            since_tick,
        },
    ));
    state
}

#[test]
fn a_run_accumulates_only_while_running() {
    let stopped = CarrierRun::STOPPED;
    assert_eq!(stopped.run_ticks_at(500), 0);
    let running = stopped.set_running(true, 100);
    assert_eq!(running.run_ticks_at(100), 0);
    assert_eq!(running.run_ticks_at(130), 30);
    let frozen = running.set_running(false, 130);
    assert_eq!(
        frozen,
        CarrierRun {
            running: false,
            run_ticks: 30,
            since_tick: 130
        }
    );
    assert_eq!(frozen.run_ticks_at(900), 30);
    let resumed = frozen.set_running(true, 900);
    assert_eq!(resumed.run_ticks_at(910), 40);
}

#[test]
fn an_unchanged_flip_keeps_its_stamp() {
    let running = CarrierRun::STOPPED.set_running(true, 100);
    assert_eq!(running.set_running(true, 150), running);
    assert_eq!(CarrierRun::STOPPED.set_running(false, 150), CarrierRun::STOPPED);
}

#[test]
fn a_stamp_ahead_of_the_tick_adds_nothing() {
    let running = CarrierRun::STOPPED.set_running(true, 100);
    assert_eq!(running.run_ticks_at(95), 0);
    let mid_leg = CarrierRun {
        running: true,
        run_ticks: 12,
        since_tick: 100,
    };
    assert_eq!(mid_leg.run_ticks_at(95), 12);
}

#[test]
fn a_switched_carrier_rests_until_its_run_says_otherwise() {
    let mut carriers = Carriers::from_layout(&MapLayout {
        carriers: vec![switched()],
        ..Default::default()
    });
    carriers.advance(30, &PlateState::default());
    assert_eq!(carriers.pose(SLIDER).translation, Vec3::ZERO);
    assert_eq!(carriers.displacement(SLIDER), Vec3::ZERO);

    carriers.advance(31, &running_since(30, 0));
    assert_eq!(carriers.pose(SLIDER).translation, carrier_offset_at(&slider(), 1));
    carriers.advance(45, &running_since(30, 0));
    assert_eq!(carriers.pose(SLIDER).translation, carrier_offset_at(&slider(), 15));
}

#[test]
fn a_switched_carrier_resumes_mid_leg_from_its_run() {
    let mut carriers = Carriers::from_layout(&MapLayout {
        carriers: vec![switched()],
        ..Default::default()
    });
    let frozen = CarrierRun {
        running: false,
        run_ticks: 20,
        since_tick: 50,
    };
    let mut state = PlateState::default();
    state.carrier_runs.push((SLIDER, frozen));
    carriers.advance(200, &state);
    carriers.advance(201, &state);
    assert_eq!(carriers.pose(SLIDER).translation, carrier_offset_at(&slider(), 20));
    assert_eq!(carriers.displacement(SLIDER), Vec3::ZERO);

    state.carrier_runs[0].1 = frozen.set_running(true, 201);
    carriers.advance(202, &state);
    assert_eq!(carriers.pose(SLIDER).translation, carrier_offset_at(&slider(), 21));
    assert!(carriers.displacement(SLIDER).x > 0.0);
}
