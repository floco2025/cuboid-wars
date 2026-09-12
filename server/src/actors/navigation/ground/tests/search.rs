use super::super::GroundNavigation;
use super::{NavGraphs, NavWaypoint};
use crate::map::ZoneVolume;
use crate::{
    actors::test_kinds::{self, CONTACT},
    map::{CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    test_geometry::{CELL, FLOOR_THICKNESS, geometry},
};
use common::{
    map::Carriers,
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
            |_, _| true,
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
            |from, to| !constrained || (volume.contains(from.into(), 0.0) && volume.contains(to.into(), 0.0)),
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
            |_, _| true,
            100,
            None,
        );
        assert_eq!(route.is_some(), gap == 0.0, "gap={gap}");
        if let Some(route) = route {
            assert_eq!(route.waypoints.back().map(|p: &NavWaypoint| p.position), Some(target));
        }
    }
}

#[test]
fn long_routes_resume_under_a_small_budget_and_failed_queries_are_cached() {
    use super::super::{GroundSearchResult, GroundState, GroundTask};
    let map = rectangle(80, 1);
    let graphs = NavGraphs::new(&map);
    let carriers = Carriers::default();
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let nav = GroundNavigation {
        graphs: &graphs,
        carriers: &carriers,
        carrier: CarrierId::WORLD,
        kind: CONTACT,
        world: &world,
        physics: test_kinds::physics(CONTACT),
        open: &[],
    };
    let start = Position {
        x: map.root_grid().geometry.cell_center_x(0),
        ..Position::default()
    };
    let target = Position {
        x: map.root_grid().geometry.cell_center_x(79),
        ..start
    };
    let mut state = GroundState::default();
    let mut found = false;
    for _ in 0..40 {
        state.tick(0.03, 4);
        let result = state.route(
            &nav,
            GroundTask::Return,
            start,
            target,
            |pos, _| (pos.distance_sq(&target) < 0.01).then_some(target),
            |_, _| true,
            None,
            None,
        );
        assert!(state.work <= 4);
        match result {
            GroundSearchResult::Found(route) => {
                assert_eq!(route.waypoints.back().map(|p| p.position), Some(target));
                found = true;
                break;
            }
            GroundSearchResult::Pending => assert_eq!(state.work, 0),
            GroundSearchResult::Unreachable => panic!("valid long route was rejected instead of resumed"),
        }
    }
    assert!(found);
    let mut finished = false;
    for _ in 0..40 {
        state.tick(0.03, 4);
        if matches!(
            state.route(
                &nav,
                GroundTask::Return,
                start,
                target,
                |_, _| None,
                |_, _| true,
                None,
                None
            ),
            GroundSearchResult::Unreachable
        ) {
            finished = true;
            break;
        }
    }
    assert!(finished);
    state.tick(0.03, 4);
    assert!(matches!(
        state.route(
            &nav,
            GroundTask::Return,
            start,
            target,
            |_, _| panic!("cached failure searched again"),
            |_, _| true,
            None,
            None
        ),
        GroundSearchResult::Unreachable
    ));
    assert_eq!(state.work, 4);
    state.tick(1.1, 4);
    assert!(matches!(
        state.route(
            &nav,
            GroundTask::Return,
            start,
            Position {
                x: target.x + 0.5,
                ..target
            },
            |_, _| None,
            |_, _| true,
            None,
            None
        ),
        GroundSearchResult::Pending
    ));
    assert_eq!(state.work, 0);
}

#[test]
fn intermediate_floor_slabs_cannot_be_routed_through() {
    let map = rectangle(3, 1);
    let graphs = NavGraphs::new(&map);
    let carriers = Carriers::default();
    let world = CollisionWorld::from_map_layout(&MapLayout {
        floors: vec![Floor {
            carrier: CarrierId::WORLD,
            level: 1,
            x1: -0.5,
            x2: 0.5,
            z1: -CELL,
            z2: CELL,
            y: 1.2,
            thickness: 0.5,
        }],
        ..Default::default()
    });
    let nav = GroundNavigation {
        graphs: &graphs,
        carriers: &carriers,
        carrier: CarrierId::WORLD,
        kind: CONTACT,
        world: &world,
        physics: test_kinds::physics(CONTACT),
        open: &[],
    };
    let target = Position {
        x: CELL,
        ..Position::default()
    };
    assert!(
        nav.route(
            Position { x: -CELL, ..target },
            |pos, _| (pos.distance_sq(&target) < 0.01).then_some(target),
            |_, _| true,
            100,
            None
        )
        .is_none()
    );
}

#[test]
fn many_unreachable_queries_keep_their_per_tick_work_limit() {
    use super::super::{GroundSearchResult, GroundState, GroundTask};
    use common::protocol::PlayerId;
    use std::{cell::Cell, time::Instant};
    let map = rectangle(40, 40);
    let graphs = NavGraphs::new(&map);
    let carriers = Carriers::default();
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let nav = GroundNavigation {
        graphs: &graphs,
        carriers: &carriers,
        carrier: CarrierId::WORLD,
        kind: CONTACT,
        world: &world,
        physics: test_kinds::physics(CONTACT),
        open: &[],
    };
    let start = Position {
        x: map.root_grid().geometry.cell_center_x(20),
        y: 0.0,
        z: map.root_grid().geometry.cell_center_z(20),
    };
    let target = Position { y: 100.0, ..start };
    let mut states: Vec<_> = (0..16).map(|_| GroundState::default()).collect();
    let elapsed = Instant::now();
    let furthest_player = Cell::new(0);
    for _ in 0..120 {
        let visited = Cell::new(0usize);
        for state in &mut states {
            state.tick(1.0 / 30.0, 16);
            for player in 1..=4 {
                let result = state.route(
                    &nav,
                    GroundTask::Pursue(PlayerId(player)),
                    start,
                    target,
                    |_, _| {
                        visited.set(visited.get() + 1);
                        furthest_player.set(furthest_player.get().max(player));
                        None
                    },
                    |_, _| true,
                    None,
                    None,
                );
                if matches!(result, GroundSearchResult::Pending) {
                    break;
                }
                assert!(matches!(result, GroundSearchResult::Unreachable));
            }
        }
        assert!(visited.get() <= 16 * 16);
    }
    assert!(furthest_player.get() >= 2);
    eprintln!(
        "16 actors, 4 players, 1600-node map, 120 unreachable-search ticks: {:?}",
        elapsed.elapsed()
    );
}
