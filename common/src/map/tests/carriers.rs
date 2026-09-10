use super::*;
use crate::{protocol::Position, test_geometry::LEVEL_HEIGHT};

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
    runtime.advance(tick.wrapping_sub(1));
    runtime.advance(tick);
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
