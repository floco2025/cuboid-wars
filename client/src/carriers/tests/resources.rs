use super::*;
use common::protocol::{Carrier, Position};

#[test]
fn tag_adds_the_carrier_base_and_motion() {
    let lift = Carrier {
        parent: CarrierId::WORLD,
        level: 2,
        levels: 1,
        from: Position::default(),
        to: Position::default(),
        travel_ticks: 1,
        pause_ticks: 0,
        phase_ticks: 0,
        switch: None,
    };
    let storeys = CarrierStoreys::from_layout(&MapLayout {
        carriers: vec![lift],
        ..Default::default()
    });

    assert_eq!(storeys.tag(CarrierId::WORLD, 1, 0), MapLevel { level: 1, span: 0 });
    assert_eq!(storeys.tag(CarrierId(1), 0, 0), MapLevel { level: 2, span: 1 });
    assert_eq!(storeys.tag(CarrierId(1), 1, 2), MapLevel { level: 3, span: 3 });
}
