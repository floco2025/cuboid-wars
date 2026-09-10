use super::*;
use crate::{
    actors::test_kinds,
    map::{CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    test_geometry::{CELL, LEVEL_HEIGHT, geometry},
};
use common::protocol::{Carrier, MapLayout};

// A world grid and a 2x2 two-storey nested grid whose carrier rests at
// x = 20 on the ground storey.
fn fixture() -> (NavGraphs, Carriers) {
    let level = |cols, rows| {
        let mut cells = CellGrid::new(cols, rows);
        for row in &mut cells.rows {
            for cell in row {
                cell.has_floor = true;
            }
        }
        LevelGrid {
            cells,
            edges: EdgeGrid::new(cols, rows),
            barrier_edges: EdgeGrid::new(cols, rows),
        }
    };
    let mut map = MapConfig::for_grid(vec![level(8, 8)], geometry(8, 8));
    map.grids.push(CarrierGrid::new(
        CarrierId(1),
        geometry(2, 2),
        vec![level(2, 2), level(2, 2)],
    ));
    let rest = Position {
        x: 20.0,
        y: 0.0,
        z: 0.0,
    };
    let carriers = Carriers::from_layout(&MapLayout {
        carriers: vec![Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: rest,
            to: rest,
            travel_ticks: 1,
            pause_ticks: 0,
            phase_ticks: 0,
        }],
        ..MapLayout::default()
    });
    (NavGraphs::new(&map), carriers)
}

fn actor(carrier: CarrierId) -> ActorInfo {
    ActorInfo::new(Entity::from_bits(1), 0, test_kinds::CONTACT.to_owned(), carrier)
}

#[test]
fn an_actor_below_its_carriers_floor_has_left_it() {
    let (graphs, carriers) = fixture();
    let below = Position {
        x: 20.0,
        y: -1.0,
        z: 0.0,
    };
    assert!(left_carrier(&actor(CarrierId(1)), &below, &carriers, &graphs));
    assert!(!left_carrier(&actor(CarrierId::WORLD), &below, &carriers, &graphs));
}

#[test]
fn an_actor_beside_its_carrier_has_left_it() {
    let (graphs, carriers) = fixture();
    let beside = Position {
        x: 20.0 + CELL * 1.5,
        y: 0.0,
        z: 0.0,
    };
    assert!(left_carrier(&actor(CarrierId(1)), &beside, &carriers, &graphs));
}

#[test]
fn an_actor_on_its_carriers_upper_storey_is_still_aboard() {
    let (graphs, carriers) = fixture();
    let upstairs = Position {
        x: 20.0,
        y: LEVEL_HEIGHT,
        z: 0.0,
    };
    assert!(!left_carrier(&actor(CarrierId(1)), &upstairs, &carriers, &graphs));
    let above_the_roof = Position {
        y: 2.0 * LEVEL_HEIGHT,
        ..upstairs
    };
    assert!(left_carrier(&actor(CarrierId(1)), &above_the_roof, &carriers, &graphs));
}
