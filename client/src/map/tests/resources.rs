use super::*;
use crate::test_fixtures::{CELL, LEVEL_HEIGHT, sizes};
use common::protocol::{Carrier, CarrierId, CarrierMotion, Position};

#[test]
fn dimensions_span_the_root_footprint_and_the_tallest_carried_storey() {
    let layout = MapLayout {
        carriers: vec![Carrier {
            initially_on: true,
            motion: CarrierMotion::Cycle,
            parent: CarrierId::WORLD,
            level: 2,
            levels: 1,
            from: Position::from(Vec3::ZERO),
            to: Position::from(Vec3::Y * LEVEL_HEIGHT),
            travel_ticks: 30,
            pause_ticks: 0,
            phase_ticks: 0,
            switch: None,
        }],
        ..Default::default()
    };
    let grids = [
        CarrierGrid {
            carrier: CarrierId::WORLD,
            cols: 10,
            rows: 8,
            levels: 3,
        },
        CarrierGrid {
            carrier: CarrierId(1),
            cols: 2,
            rows: 2,
            levels: 2,
        },
    ];

    let dimensions = MapDimensions::from_grids(&layout, &grids, sizes());

    assert_eq!(dimensions.width, 10.0 * CELL);
    assert_eq!(dimensions.depth, 8.0 * CELL);
    // Placed on storey 2, rising one more, and two storeys tall itself: past
    // the root's three.
    assert_eq!(dimensions.height, 5.0 * LEVEL_HEIGHT);
}
