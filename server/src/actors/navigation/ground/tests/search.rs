use super::super::GroundNavigation;
use super::{NavGraphs, NavWaypoint};
use crate::{
    actors::test_kinds::{self, CONTACT},
    map::{CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    test_geometry::{CELL, FLOOR_THICKNESS, geometry},
};
use common::{
    map::{Carriers, ZoneVolume},
    physics::CollisionWorld,
    protocol::{Barrier, BarrierId, BarrierKindId, Carrier, CarrierId, Floor, MapLayout, Position, Wall},
};

fn level() -> LevelGrid {
    let mut cells = CellGrid::new(1, 1);
    cells.rows[0][0].has_floor = true;
    LevelGrid {
        cells,
        edges: EdgeGrid::new(1, 1),
        barrier_edges: EdgeGrid::new(1, 1),
    }
}

fn rectangle(cols: i32, rows: i32) -> MapConfig {
    let mut cells = CellGrid::new(cols, rows);
    for cell in cells.rows.iter_mut().flatten() {
        cell.has_floor = true;
    }
    MapConfig::for_grid(
        vec![LevelGrid {
            cells,
            edges: EdgeGrid::new(cols, rows),
            barrier_edges: EdgeGrid::new(cols, rows),
        }],
        geometry(cols, rows),
    )
}

#[test]
fn closed_barriers_block_routes_and_opening_them_allows_the_same_route() {
    let map = rectangle(3, 1);
    let graphs = NavGraphs::new(&map);
    let carriers = Carriers::default();
    let world = CollisionWorld::from_map_layout(&MapLayout {
        barriers: vec![Barrier {
            id: BarrierId(1),
            kind: BarrierKindId(0),
            carrier: CarrierId::WORLD,
            switch: None,
            switch_inverted: false,
            level: 0,
            levels: 1,
            x1: 0.0,
            x2: 0.0,
            z1: -CELL,
            z2: CELL,
            y: 0.0,
            height: 10.0,
            width: 0.1,
        }],
        ..Default::default()
    });
    let target = Position {
        x: CELL,
        y: 0.0,
        z: 0.0,
    };
    for open in [vec![], vec![BarrierId(1)]] {
        let navigation = GroundNavigation {
            graphs: &graphs,
            carriers: &carriers,
            carrier: CarrierId::WORLD,
            kind: CONTACT,
            world: &world,
            physics: test_kinds::physics(CONTACT),
            open: &open,
        };
        let route = navigation.route(
            Position { x: -CELL, ..target },
            |pos, _| (pos.distance_sq(&target) < 0.001).then_some(target),
            |_| true,
            100,
            None,
        );
        assert_eq!(route.is_some(), !open.is_empty());
    }
}

#[test]
fn roaming_cannot_detour_outside_its_volume_while_pursuit_can() {
    let map = rectangle(3, 2);
    let graphs = NavGraphs::new(&map);
    let carriers = Carriers::default();
    let world = CollisionWorld::from_map_layout(&MapLayout {
        walls: vec![Wall {
            carrier: CarrierId::WORLD,
            level: 0,
            x1: -CELL / 2.0,
            x2: -CELL / 2.0,
            z1: -CELL,
            z2: 0.0,
            y: 0.0,
            height: 10.0,
            width: 0.1,
        }],
        ..Default::default()
    });
    let navigation = GroundNavigation {
        graphs: &graphs,
        carriers: &carriers,
        carrier: CarrierId::WORLD,
        kind: CONTACT,
        world: &world,
        physics: test_kinds::physics(CONTACT),
        open: &[],
    };
    let start = Position {
        x: -CELL,
        y: 0.0,
        z: -CELL / 2.0,
    };
    let target = Position { x: CELL, ..start };
    let volume = ZoneVolume::from_grid(map.root_grid().geometry, 0, 1, [0, 3], [0, 1]);
    for constrained in [true, false] {
        let route = navigation.route(
            start,
            |pos, _| (pos.distance_sq(&target) < 0.001).then_some(target),
            |pos| !constrained || volume.contains(pos.into(), 0.0),
            100,
            None,
        );
        assert_eq!(route.is_some(), !constrained);
    }
}

#[test]
fn routes_cross_connected_carriers_but_cannot_cross_an_air_gap() {
    let mut map = MapConfig::for_grid(vec![level()], geometry(1, 1));
    map.grids
        .push(CarrierGrid::new(CarrierId(1), geometry(1, 1), vec![level()]));
    let graphs = NavGraphs::new(&map);
    for gap in [0.0, 1.0] {
        let target = Position {
            x: CELL + gap,
            y: 0.0,
            z: 0.0,
        };
        let layout = MapLayout {
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 1,
                from: target,
                to: target,
                travel_ticks: 1,
                pause_ticks: 0,
                phase_ticks: 0,
                switch: None,
                switch_inverted: false,
            }],
            floors: [CarrierId::WORLD, CarrierId(1)]
                .into_iter()
                .map(|carrier| Floor {
                    x1: -CELL / 2.0,
                    x2: CELL / 2.0,
                    z1: -CELL / 2.0,
                    z2: CELL / 2.0,
                    y: 0.0,
                    thickness: FLOOR_THICKNESS,
                    level: 0,
                    carrier,
                })
                .collect(),
            ..Default::default()
        };
        let carriers = Carriers::from_layout(&layout);
        let world = CollisionWorld::from_map_layout(&layout);
        let navigation = GroundNavigation {
            graphs: &graphs,
            carriers: &carriers,
            carrier: CarrierId::WORLD,
            kind: CONTACT,
            world: &world,
            physics: test_kinds::physics(CONTACT),
            open: &[],
        };
        let route = navigation.route(
            Position::default(),
            |pos, _| (pos.distance_sq(&target) < 0.001).then_some(target),
            |_| true,
            100,
            None,
        );
        assert_eq!(route.is_some(), gap == 0.0, "gap={gap}");
        if let Some(route) = route {
            assert_eq!(route.waypoints.back().map(|p: &NavWaypoint| p.position), Some(target));
        }
    }
}
