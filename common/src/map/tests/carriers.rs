use super::*;
use crate::{
    protocol::{Position, SwitchId},
    test_geometry::LEVEL_HEIGHT,
};

fn slider() -> Carrier {
    Carrier {
        motion: Default::default(),
        switch_inverted: false,

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
    runtime.advance(tick.wrapping_sub(1), &SwitchState::default());
    runtime.advance(tick, &SwitchState::default());
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

fn running_since(since_tick: u32, run_ticks: u32) -> SwitchState {
    let mut state = SwitchState::default();
    state.carrier_runs.push((
        SLIDER,
        CarrierRun {
            active: true,
            run_ticks,
            since_tick,
        },
    ));
    state
}

#[test]
fn a_run_accumulates_only_while_running() {
    let stopped = CarrierRun::STOPPED;
    assert_eq!(stopped.run_ticks_at(500, &switched()), 0);
    let running = stopped.set_active(true, 100, &switched());
    assert_eq!(running.run_ticks_at(100, &switched()), 0);
    assert_eq!(running.run_ticks_at(130, &switched()), 30);
    let frozen = running.set_active(false, 130, &switched());
    assert_eq!(
        frozen,
        CarrierRun {
            active: false,
            run_ticks: 30,
            since_tick: 130
        }
    );
    assert_eq!(frozen.run_ticks_at(900, &switched()), 30);
    let resumed = frozen.set_active(true, 900, &switched());
    assert_eq!(resumed.run_ticks_at(910, &switched()), 40);
}

#[test]
fn an_unchanged_flip_keeps_its_stamp() {
    let running = CarrierRun::STOPPED.set_active(true, 100, &switched());
    assert_eq!(running.set_active(true, 150, &switched()), running);
    assert_eq!(
        CarrierRun::STOPPED.set_active(false, 150, &switched()),
        CarrierRun::STOPPED
    );
}

#[test]
fn a_stamp_ahead_of_the_tick_adds_nothing() {
    let running = CarrierRun::STOPPED.set_active(true, 100, &switched());
    assert_eq!(running.run_ticks_at(95, &switched()), 0);
    let mid_leg = CarrierRun {
        active: true,
        run_ticks: 12,
        since_tick: 100,
    };
    assert_eq!(mid_leg.run_ticks_at(95, &switched()), 12);
}

#[test]
fn a_switched_carrier_rests_until_its_run_says_otherwise() {
    let mut carriers = Carriers::from_layout(&MapLayout {
        carriers: vec![switched()],
        ..Default::default()
    });
    carriers.advance(30, &SwitchState::default());
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
        active: false,
        run_ticks: 20,
        since_tick: 50,
    };
    let mut state = SwitchState::default();
    state.carrier_runs.push((SLIDER, frozen));
    carriers.advance(200, &state);
    carriers.advance(201, &state);
    assert_eq!(carriers.pose(SLIDER).translation, carrier_offset_at(&slider(), 20));
    assert_eq!(carriers.displacement(SLIDER), Vec3::ZERO);

    state.carrier_runs[0].1 = frozen.set_active(true, 201, &switched());
    carriers.advance(202, &state);
    assert_eq!(carriers.pose(SLIDER).translation, carrier_offset_at(&slider(), 21));
    assert!(carriers.displacement(SLIDER).x > 0.0);
}

fn follower() -> Carrier {
    Carrier {
        motion: CarrierMotion::FollowSwitch,
        ..switched()
    }
}

#[test]
fn following_reverses_immediately_at_every_position_and_ignores_cycle_timing() {
    for pause_ticks in [0, 3, 100] {
        for phase_ticks in [0, 3, 19, u32::MAX - 5] {
            let carrier = Carrier {
                travel_ticks: 7,
                pause_ticks,
                phase_ticks,
                ..follower()
            };
            for progress in 0..=carrier.travel_ticks {
                let outward = CarrierRun::initial(&carrier).set_active(true, 100, &carrier);
                let returning = outward.set_active(false, 100 + progress, &carrier);
                assert_eq!(returning.run_ticks, progress);
                for elapsed in 0..=10 {
                    let tick = 100 + progress + elapsed;
                    let expected = progress.saturating_sub(elapsed);
                    assert_eq!(returning.run_ticks_at(tick, &carrier), expected);
                    let resumed = returning.set_active(true, tick, &carrier);
                    assert_eq!(resumed.run_ticks, expected);
                    assert_eq!(resumed.run_ticks_at(tick + 1, &carrier), (expected + 1).min(7));
                    assert_eq!(resumed.run_ticks_at(tick + 100, &carrier), 7);
                    assert_eq!(
                        carrier_offset_at(&carrier, resumed.run_ticks_at(tick + 100, &carrier)),
                        Vec3::from(carrier.to)
                    );
                    assert_eq!(resumed.set_active(true, tick + 5, &carrier), resumed);
                }
            }
        }
    }
}

#[test]
fn following_starts_at_the_endpoint_selected_by_the_initial_off_switch() {
    for inverted in [false, true] {
        let carrier = Carrier {
            switch_inverted: inverted,
            phase_ticks: 30,
            ..follower()
        };
        let mut carriers = Carriers::from_layout(&MapLayout {
            carriers: vec![carrier],
            ..Default::default()
        });
        let start = if inverted {
            Vec3::from(carrier.to)
        } else {
            Vec3::from(carrier.from)
        };
        assert_eq!(carriers.pose(SLIDER).translation, start);
        carriers.advance(100, &SwitchState::default());
        assert_eq!(carriers.pose(SLIDER).translation, start);
        let run = CarrierRun::initial(&carrier).set_active(!inverted, 100, &carrier);
        let next = carrier_offset_at(&carrier, run.run_ticks_at(101, &carrier));
        assert!(((next - start).length() - 4.0 / 60.0).abs() < 1e-5);
    }
}

#[test]
fn a_replicated_return_handles_tick_wrap_and_a_trailing_clock() {
    let carrier = Carrier {
        motion: crate::protocol::CarrierMotion::FollowSwitch,
        ..switched()
    };
    let returning = CarrierRun::initial(&carrier)
        .set_active(true, u32::MAX - 30, &carrier)
        .set_active(false, u32::MAX - 10, &carrier);
    let state = SwitchState {
        carrier_runs: vec![(SLIDER, returning)],
        ..Default::default()
    };
    let bytes = bincode::encode_to_vec(&state, bincode::config::standard()).expect("encode return state");
    let (decoded, _): (SwitchState, _) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("decode return state");
    let layout = MapLayout {
        carriers: vec![
            carrier,
            Carrier {
                parent: SLIDER,
                ..lift()
            },
        ],
        ..Default::default()
    };
    let mut server = Carriers::from_layout(&layout);
    let mut client = Carriers::from_layout(&layout);
    for tick in [u32::MAX - 12, u32::MAX - 10, u32::MAX, 9, 100] {
        server.advance(tick, &state);
        client.advance(tick, &decoded);
        assert_eq!(server.pose(SLIDER), client.pose(SLIDER));
        assert_eq!(server.pose(CarrierId(2)), client.pose(CarrierId(2)));
    }
    assert_eq!(server.pose(SLIDER).translation, Vec3::ZERO);
    assert_eq!(returning.run_ticks_at(u32::MAX - 12, &carrier), returning.run_ticks);
}
