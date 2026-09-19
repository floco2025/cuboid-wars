use super::super::{GroundNavigation, GroundSearchOptions};
use super::{GroundSearchResult, NavGraphs, NavWaypoint};
use crate::map::ZoneVolume;
use crate::{
    actors::test_kinds::{self, CONTACT},
    map::{CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    test_geometry::{CELL, FLOOR_THICKNESS, WALL_THICKNESS, geometry},
};
use common::{
    map::{Carriers, Grounds, GroundsSettings},
    physics::CollisionWorld,
    protocol::{Barrier, Carrier, CarrierId, FieldId, Floor, MapLayout, Position, Wall},
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
            field: FieldId(0),
            carrier: CarrierId::WORLD,
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
    for open in [vec![], vec![FieldId(0)]] {
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
                motion: Default::default(),
                parent: CarrierId::WORLD,
                level: 0,
                levels: 1,
                from: target,
                to: target,
                travel_ticks: 1,
                pause_ticks: 0,
                phase_ticks: 0,
                switch: None,
                initially_on: true,
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
            GroundSearchOptions::default(),
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
                GroundSearchOptions::default()
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
            GroundSearchOptions::default()
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
            GroundSearchOptions::default()
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
                    GroundSearchOptions::default(),
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

#[test]
fn routes_walk_out_onto_the_grounds() {
    let map = rectangle(2, 1);
    let geometry = map.root_grid().geometry;
    let half_width = geometry.width() / 2.0 + WALL_THICKNESS / 2.0;
    let half_depth = geometry.depth() / 2.0 + WALL_THICKNESS / 2.0;
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -half_width,
            z1: -half_depth,
            x2: half_width,
            z2: half_depth,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        grounds: Some(Grounds::new(
            [(-half_width, half_width, -half_depth, half_depth)],
            0.0,
            GroundsSettings { level: 0 },
        )),
        ..Default::default()
    };
    let mut graphs = NavGraphs::new(&map);
    graphs.add_grounds(&layout);
    let world = CollisionWorld::from_map_layout(&layout);
    let carriers = Carriers::default();
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
        x: geometry.cell_center_x(0),
        y: 0.0,
        z: 0.0,
    };
    let target = Position {
        x: geometry.cell_center_x(2),
        y: 0.0,
        z: 0.0,
    };
    let route = navigation
        .route(
            start,
            |pos, _| (pos.distance_sq(&target) < 0.001).then_some(target),
            |_, _| true,
            100,
            None,
        )
        .expect("the grounds continue the floor past the map's edge");
    assert_eq!(route.waypoints.back().map(|point| point.position), Some(target));
    assert!(target.x > half_width, "the target lies past the map's edge");
}

#[test]
fn a_goal_estimate_reaches_a_far_target_within_a_budget_a_plain_search_exhausts() {
    let map = rectangle(20, 20);
    let geometry = map.root_grid().geometry;
    let graphs = NavGraphs::new(&map);
    let carriers = Carriers::default();
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
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
        x: geometry.cell_center_x(0),
        y: 0.0,
        z: geometry.cell_center_z(0),
    };
    let target = Position {
        x: geometry.cell_center_x(19),
        y: 0.0,
        z: geometry.cell_center_z(19),
    };
    let goal = |pos: Position, _| (pos.distance_sq(&target) < 0.001).then_some(target);
    let estimate = |pos: Position| (target.x - pos.x).abs() + (target.z - pos.z).abs();

    let mut work = 200;
    let mut search = navigation.search(start).expect("start cell missing");
    let result = navigation.advance(
        &mut search,
        goal,
        |_, _| true,
        &mut work,
        GroundSearchOptions {
            heuristic: Some(&estimate),
            ..Default::default()
        },
    );
    assert!(matches!(result, GroundSearchResult::Found(_)));

    let mut work = 200;
    let mut search = navigation.search(start).expect("start cell missing");
    let result = navigation.advance(
        &mut search,
        goal,
        |_, _| true,
        &mut work,
        GroundSearchOptions::default(),
    );
    assert!(matches!(result, GroundSearchResult::Pending));
}

#[test]
fn a_joined_route_crosses_open_floor_on_one_leg_and_turns_at_a_wall() {
    let map = rectangle(6, 3);
    let geometry = map.root_grid().geometry;
    let graphs = NavGraphs::new(&map);
    let carriers = Carriers::default();
    let floor = Floor {
        x1: -geometry.width() / 2.0,
        z1: -geometry.depth() / 2.0,
        x2: geometry.width() / 2.0,
        z2: geometry.depth() / 2.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    let open = CollisionWorld::from_map_layout(&MapLayout {
        floors: vec![floor],
        ..Default::default()
    });
    let navigation = |world| GroundNavigation {
        graphs: &graphs,
        carriers: &carriers,
        carrier: CarrierId::WORLD,
        kind: CONTACT,
        world,
        physics: test_kinds::physics(CONTACT),
        open: &[],
    };
    let start = Position {
        x: geometry.cell_center_x(0),
        y: 0.0,
        z: geometry.cell_center_z(0),
    };
    let target = Position {
        x: geometry.cell_center_x(5),
        y: 0.0,
        z: geometry.cell_center_z(2),
    };
    let route_to_target = |navigation: &GroundNavigation<'_>| {
        let mut route = navigation
            .route(
                start,
                |pos, _| (pos.distance_sq(&target) < 0.001).then_some(target),
                |_, _| true,
                1000,
                None,
            )
            .expect("an open floor is walkable");
        assert!(navigation.join_route(start, &mut route, &|_, _| true));
        route
    };

    let straight = route_to_target(&navigation(&open));
    assert_eq!(
        straight
            .waypoints
            .iter()
            .map(|point| point.position)
            .collect::<Vec<_>>(),
        vec![target],
        "open floor is crossed on one diagonal leg"
    );

    // A wall across the middle row from the north edge, open at the south.
    let walled = CollisionWorld::from_map_layout(&MapLayout {
        floors: vec![floor],
        walls: vec![Wall {
            carrier: CarrierId::WORLD,
            level: 0,
            x1: geometry.cell_to_world_x(3),
            x2: geometry.cell_to_world_x(3),
            z1: geometry.cell_to_world_z(0),
            z2: geometry.cell_to_world_z(2),
            y: 0.0,
            height: 10.0,
            width: WALL_THICKNESS,
        }],
        ..Default::default()
    });
    let around = route_to_target(&navigation(&walled));
    assert!(around.waypoints.len() >= 2, "the wall forces a turn");
    assert!(
        around.waypoints.len() <= 3,
        "legs on either side of the wall's end are straight: {:?}",
        around.waypoints
    );
    assert_eq!(around.waypoints.back().map(|point| point.position), Some(target));
}
