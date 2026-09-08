use std::collections::VecDeque;

use bevy::prelude::Vec3;
use common::{
    config::CharacterPhysicsConfig,
    constants::TICK_SECS,
    map::Carriers,
    physics::{CharacterEnvironment, CharacterStep, CollisionWorld, LadderMode, step_character_movement},
    protocol::{BarrierKindTable, Carrier, CarrierId, Floor, Ladder, MapLayout, Position, Wall},
};
use rand::{SeedableRng, rngs::StdRng};

use super::{ActorTerritories, NavGraph, NavGraphs, NavNode, WaypointKind};
use crate::{
    config::ServerGameplayConfig,
    map::{ActorSpawnZone, CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    test_geometry::{CELL, FLOOR_THICKNESS, LEVEL_HEIGHT, geometry},
};

struct Fixture {
    graph: NavGraph,
    layout: MapLayout,
    physics: CharacterPhysicsConfig,
}

impl Fixture {
    fn new(storeys: u8, intermediate: bool, transpose: bool) -> Self {
        let (cols, rows) = if transpose { (2, 1) } else { (1, 2) };
        let mut levels = Vec::new();
        let mut floors = Vec::new();
        for level in 0..=storeys {
            let mut cells = CellGrid::new(cols, rows);
            let landing = level == 0 || level == storeys || intermediate;
            if landing {
                let side = usize::from(level != 0);
                let (row, col) = if transpose { (0, side) } else { (side, 0) };
                cells.rows[row][col].has_floor = true;
                let mut floor = Floor {
                    x1: -CELL / 2.0,
                    x2: CELL / 2.0,
                    z1: if level == 0 { -CELL } else { 0.0 },
                    z2: if level == 0 { 0.0 } else { CELL },
                    y: f32::from(level) * LEVEL_HEIGHT,
                    thickness: FLOOR_THICKNESS,
                    level,
                    carrier: CarrierId::WORLD,
                };
                if transpose {
                    (floor.x1, floor.z1) = (floor.z1, floor.x1);
                    (floor.x2, floor.z2) = (floor.z2, floor.x2);
                }
                floors.push(floor);
            }
            levels.push(LevelGrid {
                cells,
                edges: EdgeGrid::new(cols, rows),
                barrier_edges: EdgeGrid::new(cols, rows),
            });
        }
        let mut ladder = Ladder {
            x1: -0.6,
            x2: 0.6,
            z1: 0.0,
            z2: 0.0,
            nx: 0.0,
            nz: -1.0,
            y: 0.0,
            height: f32::from(storeys) * LEVEL_HEIGHT,
            level: 0,
            levels: storeys,
            carrier: CarrierId::WORLD,
        };
        if transpose {
            (ladder.x1, ladder.z1) = (ladder.z1, ladder.x1);
            (ladder.x2, ladder.z2) = (ladder.z2, ladder.x2);
            (ladder.nx, ladder.nz) = (ladder.nz, ladder.nx);
        }
        Self {
            graph: NavGraph::new(&CarrierGrid::new(CarrierId::WORLD, geometry(cols, rows), levels)),
            layout: MapLayout {
                ladders: vec![ladder],
                floors,
                ..Default::default()
            },
            physics: ServerGameplayConfig::load_default()
                .expect("default gameplay missing")
                .gameplay_config()
                .expect_actor("mine")
                .physics(),
        }
    }

    fn reflected(mut self) -> Self {
        let mut levels = self.graph.levels.clone();
        for level in &mut levels {
            level.cells.rows.reverse();
            for row in &mut level.cells.rows {
                row.reverse();
            }
        }
        self.graph = NavGraph::new(&CarrierGrid::new(CarrierId::WORLD, self.graph.geometry, levels));
        for floor in &mut self.layout.floors {
            (floor.x1, floor.x2) = (-floor.x2, -floor.x1);
            (floor.z1, floor.z2) = (-floor.z2, -floor.z1);
        }
        for ladder in &mut self.layout.ladders {
            ladder.x1 = -ladder.x1;
            ladder.x2 = -ladder.x2;
            ladder.z1 = -ladder.z1;
            ladder.z2 = -ladder.z2;
            ladder.nx = -ladder.nx;
            ladder.nz = -ladder.nz;
        }
        self
    }

    fn links(&self) -> Vec<super::LadderLink> {
        let world = CollisionWorld::from_map_layout(&self.layout, &BarrierKindTable::default());
        let carriers = Carriers::default();
        self.graph.build_ladder_links(
            &self.layout.ladders[0],
            &CharacterEnvironment {
                ladder_mode: LadderMode::Disabled,
                collision_world: &world,
                gravity: 25.0,
                passable_kinds: &[],
                physics: self.physics,
                ladder_climb_ratio: 0.4,
                portals: None,
                carriers: &carriers,
            },
            [3.0, 5.0],
        )
    }
}

#[test]
fn ladders_connect_floor_components_in_both_directions() {
    for transpose in [false, true] {
        for fixture in [
            Fixture::new(1, false, transpose),
            Fixture::new(1, false, transpose).reflected(),
        ] {
            let links = fixture.links();
            assert_eq!(links.len(), 2, "links: {links:?}");
            for link in &links {
                let start = fixture.graph.node_center(link.from);
                let target = fixture.graph.node_center(link.to);
                assert!(fixture.graph.engagement_route(&[], &start, &target, 0.5, 0.5).is_none());
                let route = fixture
                    .graph
                    .engagement_route(&links, &start, &target, 0.5, 0.5)
                    .expect("ladder route missing");
                assert!(
                    route
                        .waypoints
                        .iter()
                        .any(|point| matches!(point.kind, WaypointKind::Mount))
                );
                assert!(
                    route
                        .waypoints
                        .iter()
                        .any(|point| matches!(point.kind, WaypointKind::Climb { .. }))
                );
                assert!(
                    route
                        .waypoints
                        .iter()
                        .any(|point| matches!(point.kind, WaypointKind::Exit))
                );
            }
        }
    }
}

#[test]
fn multi_storey_ladders_connect_intermediate_landings_and_span_empty_storeys() {
    for intermediate in [false, true] {
        let fixture = Fixture::new(2, intermediate, false);
        let links = fixture.links();
        assert_eq!(links.len(), if intermediate { 4 } else { 2 }, "links: {links:?}");
    }
}

#[test]
fn ladder_pursuit_reaches_a_free_end_and_returns_to_the_last_landing() {
    for single_landing in [false, true] {
        let mut fixture = Fixture::new(u8::from(!single_landing), false, false);
        fixture.layout.ladders[0].levels = 2;
        fixture.layout.ladders[0].height = 2.0 * LEVEL_HEIGHT;
        let links = fixture.links();
        let start = fixture.graph.node_center(NavNode {
            level: 0,
            row: 0,
            col: 0,
        });
        let target = Position {
            x: 0.0,
            y: 1.5 * LEVEL_HEIGHT,
            z: -0.77,
        };
        let route = fixture
            .graph
            .ladder_target_route(&start, &target, &links, fixture.physics)
            .expect("pursuit route to the ladder's free end missing");
        assert_eq!(
            route.waypoints.back().expect("pursuit endpoint missing").position.y,
            target.y
        );
        let retreat = fixture
            .graph
            .ladder_exit_route(&target, &links)
            .expect("return to a ladder landing missing");
        assert_eq!(retreat.destination_node.level, u8::from(!single_landing));
        assert!(matches!(
            retreat.waypoints[0].kind,
            WaypointKind::Climb { ascending: false, .. }
        ));

        fixture.layout.walls.push(Wall {
            x1: -CELL,
            x2: CELL,
            z1: -0.8,
            z2: -0.8,
            y: LEVEL_HEIGHT * 1.25,
            height: 1.0,
            width: 0.2,
            level: 1,
            carrier: CarrierId::WORLD,
        });
        assert!(
            fixture
                .graph
                .ladder_target_route(&start, &target, &fixture.links(), fixture.physics)
                .is_none()
        );
    }
}

#[test]
fn blocked_climbs_and_bodies_that_cannot_fit_have_no_route() {
    let mut fixture = Fixture::new(1, false, false);
    fixture.layout.walls.push(Wall {
        x1: -CELL,
        x2: CELL,
        z1: -0.8,
        z2: -0.8,
        width: 0.2,
        y: 1.5,
        height: 1.0,
        level: 0,
        carrier: CarrierId::WORLD,
    });
    assert!(fixture.links().is_empty());
    fixture.layout.walls.clear();
    fixture.physics.movement_collider.diameter = 3.0;
    fixture.physics.movement_collider.height = 3.0;
    assert!(fixture.links().is_empty());
}

#[test]
fn pursuit_can_end_on_a_ladder_and_replan_to_a_landing() {
    let fixture = Fixture::new(1, false, false);
    let links = fixture.links();
    let ascending = links
        .iter()
        .find(|link| link.to.level == 1)
        .expect("ascending route missing");
    let start = fixture.graph.node_center(ascending.from);
    let target = Position {
        y: LEVEL_HEIGHT / 2.0,
        ..ascending.waypoints[1].position
    };
    let route = fixture
        .graph
        .ladder_target_route(&start, &target, &links, fixture.physics)
        .expect("ladder pursuit missing");
    assert_eq!(
        route.waypoints.back().expect("route endpoint missing").position.y,
        target.y
    );
    let exit = fixture.graph.ladder_exit(&target, &links).expect("ladder exit missing");
    let destination = fixture.graph.node_center(exit.to);
    let mut route = fixture
        .graph
        .engagement_route(&links, &target, &destination, 0.5, 0.5)
        .expect("return route missing");
    assert!(fixture.graph.prepend_ladder_exit(&target, &mut route, &links));
    assert!(matches!(route.waypoints[0].kind, WaypointKind::Climb { .. }));
    assert!(
        fixture
            .graph
            .ladder_target_route(
                &start,
                &Position::from(Vec3::new(0.0, target.y, 0.8)),
                &links,
                fixture.physics
            )
            .is_none()
    );
}

#[test]
fn actors_complete_ladder_routes_on_moving_carriers() {
    for travel in [Vec3::ZERO, Vec3::new(27.0, 6.0, -12.0), Vec3::new(-27.0, -6.0, 12.0)] {
        for phase_ticks in [30, 240] {
            for intermediate in [false, true] {
                let mut fixture = Fixture::new(2, intermediate, false);
                let links = fixture.links();
                fixture.layout.carriers.push(Carrier {
                    parent: CarrierId::WORLD,
                    level: 0,
                    levels: 0,
                    from: Position::default(),
                    to: travel.into(),
                    travel_ticks: 180,
                    pause_ticks: 30,
                    phase_ticks,
                });
                for floor in &mut fixture.layout.floors {
                    floor.carrier = CarrierId(1);
                }
                fixture.layout.ladders[0].carrier = CarrierId(1);
                for link in links {
                    for speed in [3.0, 5.0] {
                        let mut carriers = Carriers::from_layout(&fixture.layout);
                        let mut world = CollisionWorld::from_map_layout(&fixture.layout, &BarrierKindTable::default());
                        let mut pos = carriers
                            .pose(CarrierId(1))
                            .transform_position(&fixture.graph.node_center(link.from));
                        let mut vertical_velocity = 0.0;
                        let mut remaining: VecDeque<_> = link.waypoints.iter().copied().collect();
                        for tick in 1..=600 {
                            let local = carriers.pose(CarrierId(1)).inverse_transform_position(&pos);
                            while remaining.front().is_some_and(|waypoint| waypoint.reached(&local, 0.15)) {
                                remaining.pop_front();
                            }
                            let Some(&waypoint) = remaining.front() else { break };
                            let intent = waypoint.movement_intent(&local, speed);
                            carriers.advance(tick);
                            world.set_carrier_poses(&carriers);
                            let step = step_character_movement(
                                CharacterStep {
                                    start: pos,
                                    vertical_velocity,
                                    control_velocity: intent.to_horizontal_velocity(),
                                    external_displacement: Vec3::ZERO,
                                    delta: TICK_SECS,
                                },
                                &CharacterEnvironment {
                                    collision_world: &world,
                                    gravity: 25.0,
                                    passable_kinds: &[],
                                    physics: fixture.physics,
                                    ladder_climb_ratio: 0.4,
                                    ladder_mode: LadderMode::for_actor(true, intent),
                                    portals: None,
                                    carriers: &carriers,
                                },
                            );
                            assert!(!step.crushed);
                            assert!(!step.blocked, "blocked on {waypoint:?} from {local:?}");
                            pos = step.position;
                            vertical_velocity = step.vertical_velocity;
                        }
                        let local = carriers.pose(CarrierId(1)).inverse_transform_position(&pos);
                        assert!(
                            remaining.is_empty(),
                            "route failed with travel {travel:?} phase {phase_ticks} speed {speed}: {local:?}, next {remaining:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn permissions_control_graph_links_and_roam_territories_per_kind() {
    let fixture = Fixture::new(1, false, false);
    let mut map = MapConfig::for_grid(fixture.graph.levels.clone(), fixture.graph.geometry);
    for kind in ["mine", "zapper"] {
        map.actor_spawn_zones.push(ActorSpawnZone {
            carrier: CarrierId::WORLD,
            level: 0,
            cols: [0, 1],
            rows: [0, 1],
            kind: kind.into(),
            count: 1,
        });
    }
    let mut config = ServerGameplayConfig::load_default().expect("default gameplay missing");
    for actor in config.actors.kinds.values_mut() {
        actor.character.can_use_ladders = false;
    }
    config
        .actors
        .kinds
        .get_mut("mine")
        .expect("mine kind missing")
        .character
        .can_use_ladders = true;
    let settings = &config.maps[&config.default_map].settings;
    let mut graphs = NavGraphs::new(&map);
    graphs.add_ladder_routes(&fixture.layout, settings, &config);
    let graph = graphs.get(CarrierId::WORLD);
    let ladders = graph.ladder_links("mine");
    assert_eq!(ladders.len(), 2);
    assert!(graph.ladder_links("zapper").is_empty());
    let territories = ActorTerritories::new(&graphs, &map, &config).expect("ladder territories invalid");
    assert_eq!(territories.get(0).roam.len(), 2);
    assert_eq!(territories.get(1).roam.len(), 1);
    let link = ladders
        .iter()
        .find(|link| link.from.level == 0)
        .expect("ascending ladder route missing");
    let start = graph.node_center(link.from);
    let mut rng = StdRng::seed_from_u64(1);
    assert!(
        graph
            .roam_route(ladders, &start, territories.get(0), &mut rng)
            .is_some()
    );
    assert!(graph.flee_route(ladders, &start, &[start], &mut rng).is_some());
    let mut home = territories.get(0).clone();
    home.roam.remove(&link.to);
    home.roam_nodes.retain(|node| *node != link.to);
    assert!(
        graph
            .return_route(ladders, &graph.node_center(link.to), &home)
            .is_some()
    );
}

#[test]
fn ladder_targets_hold_a_stable_height_and_cannot_skip_blocked_spans() {
    let fixture = Fixture::new(2, true, false);
    let mut links = fixture.links();
    let lower = links
        .iter()
        .find(|link| link.from.level == 0)
        .expect("ascending ladder route missing");
    let start = Position {
        y: 2.05,
        ..lower.waypoints[1].position
    };
    let target = Position { y: 2.0, ..start };
    let route = fixture
        .graph
        .ladder_target_route(&start, &target, &links, fixture.physics)
        .expect("ladder pursuit missing");
    assert_eq!(
        route
            .waypoints
            .back()
            .expect("pursuit endpoint missing")
            .movement_intent(&start, 5.0)
            .speed(),
        Some(0.0)
    );
    links.retain(|link| link.from.level < 2 && link.to.level < 2);
    let unreachable = Position {
        y: LEVEL_HEIGHT * 1.5,
        ..target
    };
    assert!(
        fixture
            .graph
            .ladder_target_route(&start, &unreachable, &links, fixture.physics)
            .is_none()
    );
}

#[test]
fn ladder_links_belong_to_their_carrier_grid() {
    let mut fixture = Fixture::new(1, false, false);
    let mut map = MapConfig::for_grid(fixture.graph.levels.clone(), fixture.graph.geometry);
    map.grids.push(CarrierGrid::new(
        CarrierId(1),
        fixture.graph.geometry,
        fixture.graph.levels.clone(),
    ));
    for floor in &mut fixture.layout.floors {
        floor.carrier = CarrierId(1);
    }
    fixture.layout.ladders[0].carrier = CarrierId(1);
    let mut config = ServerGameplayConfig::load_default().expect("default gameplay missing");
    config
        .actors
        .kinds
        .get_mut("mine")
        .expect("mine kind missing")
        .character
        .can_use_ladders = true;
    let mut graphs = NavGraphs::new(&map);
    graphs.add_ladder_routes(&fixture.layout, &config.maps[&config.default_map].settings, &config);
    assert!(graphs.get(CarrierId::WORLD).ladder_links("mine").is_empty());
    assert_eq!(graphs.get(CarrierId(1)).ladder_links("mine").len(), 2);
}
