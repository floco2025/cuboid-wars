use super::*;

fn carrier(parent: CarrierId, level: u8, levels: u8) -> Carrier {
    Carrier {
        switch_inverted: false,

        parent,
        level,
        levels,
        from: Position::default(),
        to: Position::default(),
        travel_ticks: 1,
        pause_ticks: 0,
        phase_ticks: 0,
        switch: None,
    }
}

#[test]
fn carrier_base_level_sums_the_parent_chain() {
    let layout = MapLayout {
        carriers: vec![carrier(CarrierId::WORLD, 2, 0), carrier(CarrierId(1), 1, 0)],
        ..Default::default()
    };
    assert_eq!(layout.carrier_base_level(CarrierId::WORLD), 0);
    assert_eq!(layout.carrier_base_level(CarrierId(1)), 2);
    assert_eq!(layout.carrier_base_level(CarrierId(2)), 3);
}

#[test]
fn carrier_motion_levels_sum_the_parent_chain() {
    let layout = MapLayout {
        carriers: vec![carrier(CarrierId::WORLD, 0, 1), carrier(CarrierId(1), 0, 2)],
        ..Default::default()
    };
    assert_eq!(layout.carrier_motion_levels(CarrierId::WORLD), 0);
    assert_eq!(layout.carrier_motion_levels(CarrierId(1)), 1);
    assert_eq!(layout.carrier_motion_levels(CarrierId(2)), 3);
}
