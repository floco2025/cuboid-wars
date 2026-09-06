use bevy::prelude::Vec3;
use common::physics::CollisionWorld;
use common::protocol::{BarrierKindTable, MapLayout, Position, Wall};

use super::{NavGraph, routing::COVER_SEARCH_MAX_STEPS};
use crate::map::{ActorSpawnZone, CellGrid, EdgeGrid, GeneratedMap, LevelGrid, MapConfig};
use crate::test_geometry::{CELL, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS, geometry};
use common::protocol::CarrierId;

fn level(cells: CellGrid, edges: EdgeGrid) -> LevelGrid {
    let rows = i32::try_from(cells.rows.len()).unwrap_or(0);
    let cols = i32::try_from(cells.rows.first().map_or(0, Vec::len)).unwrap_or(0);
    LevelGrid {
        cells,
        edges,
        barrier_edges: EdgeGrid::new(cols, rows),
    }
}

fn zone(level: u8, col: i32, row: i32) -> ActorSpawnZone {
    ActorSpawnZone {
        level,
        cols: [col, col + 1],
        rows: [row, row + 1],
        kind: "zapper".into(),
        count: 1,
    }
}

fn full_floor_nav(cols: i32, rows: i32) -> NavGraph {
    let mut cells = CellGrid::new(cols, rows);
    for row in &mut cells.rows {
        for cell in row {
            cell.has_floor = true;
        }
    }
    NavGraph::new(MapConfig::for_grid(
        vec![level(cells, EdgeGrid::new(cols, rows))],
        geometry(cols, rows),
    ))
}

fn cell_center(cols: i32, rows: i32, col: i32, row: i32) -> Position {
    let geometry = geometry(cols, rows);
    Position {
        x: geometry.cell_center_x(col),
        y: 0.0,
        z: geometry.cell_center_z(row),
    }
}

#[test]
fn engagement_route_is_direct_across_open_floor() {
    let nav = full_floor_nav(10, 5);
    let start = cell_center(10, 5, 1, 1);
    let target = cell_center(10, 5, 8, 3);

    let route = nav
        .engagement_route(&start, &target, 0.9, 0.9)
        .expect("open-floor target should be reachable");

    assert_eq!(route.waypoints.len(), 1);
    assert_eq!(route.waypoints.front(), Some(&target));
}

#[test]
fn engagement_route_keeps_a_turn_around_a_wall() {
    let mut cells = CellGrid::new(3, 2);
    for row in &mut cells.rows {
        for cell in row {
            cell.has_floor = true;
        }
    }
    let mut edges = EdgeGrid::new(3, 2);
    edges.vertical[0][1] = true;
    let nav = NavGraph::new(MapConfig::for_grid(vec![level(cells, edges)], geometry(3, 2)));
    let start = cell_center(3, 2, 0, 0);
    let target = cell_center(3, 2, 2, 0);

    let route = nav
        .engagement_route(&start, &target, 0.5, 0.5)
        .expect("target should be reachable around the wall");

    assert!(route.waypoints.len() > 1);
    assert_ne!(route.waypoints.front(), Some(&target));
    assert_eq!(route.waypoints.back(), Some(&target));
}

#[test]
fn engagement_retarget_rejects_blocked_flat_final_leg() {
    let mut cells = CellGrid::new(2, 1);
    for cell in &mut cells.rows[0] {
        cell.has_floor = true;
    }
    let mut edges = EdgeGrid::new(2, 1);
    edges.vertical[0][1] = true;
    let nav = NavGraph::new(MapConfig::for_grid(vec![level(cells, edges)], geometry(2, 1)));
    let start = cell_center(2, 1, 0, 0);
    let target = cell_center(2, 1, 1, 0);

    assert!(!nav.engagement_retarget_is_valid(&start, &target, 0.5, 0.5));
}

#[test]
fn cover_route_chooses_nearest_adequate_cover() {
    let nav = full_floor_nav(6, 1);
    let threat = cell_center(6, 1, 0, 0);
    let start = cell_center(6, 1, 2, 0);
    let near_cover = cell_center(6, 1, 3, 0);
    let far_cover = cell_center(6, 1, 5, 0);

    let route = nav
        .safe_cover_route(&start, &[threat], |candidate| {
            *candidate == near_cover || *candidate == far_cover
        })
        .expect("both cover nodes should be reachable");

    assert_eq!(route.waypoints.back(), Some(&near_cover));
}

#[test]
fn actor_at_safest_reachable_point_does_not_leave_without_cover() {
    let nav = full_floor_nav(5, 1);
    let threat = cell_center(5, 1, 0, 0);
    let start = cell_center(5, 1, 4, 0);

    let route = nav.safe_cover_route(&start, &[threat], |_| false);

    assert!(route.is_none());
}

#[test]
fn cover_search_stays_within_its_local_route_budget() {
    let nav = full_floor_nav(30, 1);
    let threat = cell_center(30, 1, 0, 0);
    let start = cell_center(30, 1, 1, 0);
    let remote_cover = cell_center(30, 1, 20, 0);

    let route = nav
        .safe_cover_route(&start, &[threat], |candidate| *candidate == remote_cover)
        .expect("the local safest-point fallback should produce a route");

    assert!(route.waypoints.len() <= COVER_SEARCH_MAX_STEPS);
    assert_ne!(route.waypoints.back(), Some(&remote_cover));
}

#[test]
fn cover_route_does_not_cross_the_threat_for_an_equivalent_destination() {
    let nav = full_floor_nav(5, 3);
    let start = cell_center(5, 3, 2, 0);
    let threat = cell_center(5, 3, 2, 1);
    let across_threat = cell_center(5, 3, 2, 2);
    let safe_side = cell_center(5, 3, 4, 0);

    let route = nav
        .safe_cover_route(&start, &[threat], |candidate| {
            *candidate == across_threat || *candidate == safe_side
        })
        .expect("a threat-avoiding cover route should exist");

    assert_eq!(route.waypoints.back(), Some(&safe_side));
    assert!(!route.waypoints.contains(&threat));
}

#[test]
fn cover_route_uses_world_occlusion() {
    let mut cells = CellGrid::new(2, 2);
    for row in &mut cells.rows {
        for cell in row {
            cell.has_floor = true;
        }
    }
    let mut edges = EdgeGrid::new(2, 2);
    edges.vertical[0][1] = true;
    let nav = NavGraph::new(MapConfig::for_grid(vec![level(cells, edges)], geometry(2, 2)));
    let world = CollisionWorld::from_map_layout(
        &MapLayout {
            walls: vec![Wall {
                x1: 0.0,
                z1: -4.0,
                x2: 0.0,
                z2: 0.0,
                width: WALL_THICKNESS,
                level: 0,
                y: 0.0,
                height: WALL_HEIGHT,
                carrier: CarrierId::WORLD,
            }],
            ..MapLayout::default()
        },
        &BarrierKindTable::default(),
    );
    let threat = cell_center(2, 2, 0, 0);
    let start = cell_center(2, 2, 0, 1);
    let actor_eye = |candidate: &Position| Vec3::new(candidate.x, candidate.y + 0.8, candidate.z);
    let player_center = Vec3::new(threat.x, threat.y + 0.8, threat.z);

    let route = nav
        .safe_cover_route(&start, &[threat], |candidate| {
            !world.line_of_sight_clear(actor_eye(candidate), player_center)
        })
        .expect("cover behind the wall is reachable around its end");

    assert!(!world.line_of_sight_clear(
        actor_eye(route.waypoints.back().expect("cover route has a destination")),
        player_center,
    ));
}

#[test]
fn path_avoids_walls() {
    let mut cells = CellGrid::new(2, 2);
    for row in &mut cells.rows {
        for cell in row {
            cell.has_floor = true;
        }
    }
    let mut edges = EdgeGrid::new(2, 2);
    edges.vertical[0][1] = true;
    let nav = NavGraph::new(MapConfig::for_grid(vec![level(cells, edges)], geometry(2, 2)));

    let path = nav
        .path_to_spawn_zone(
            &Position {
                x: -2.0,
                y: 0.0,
                z: -2.0,
            },
            &zone(0, 1, 0),
        )
        .expect("target should be reachable around the wall");

    assert!(path.len() > 1, "path should route around the blocked east edge");
}

#[test]
fn path_routes_around_closed_barrier() {
    let mut cells = CellGrid::new(2, 2);
    for row in &mut cells.rows {
        for cell in row {
            cell.has_floor = true;
        }
    }
    // Same blocked east edge as `path_avoids_walls`, but as a barrier
    // rather than a wall — actors must still route around it.
    let mut barrier_edges = EdgeGrid::new(2, 2);
    barrier_edges.vertical[0][1] = true;
    let nav = NavGraph::new(MapConfig::for_grid(
        vec![LevelGrid {
            cells,
            edges: EdgeGrid::new(2, 2),
            barrier_edges,
        }],
        geometry(2, 2),
    ));

    let path = nav
        .path_to_spawn_zone(
            &Position {
                x: -2.0,
                y: 0.0,
                z: -2.0,
            },
            &zone(0, 1, 0),
        )
        .expect("target should be reachable around the barrier");

    assert!(path.len() > 1, "path should route around the closed barrier edge");
}

// 3×3 single level, all floor, with a one-cell ramp wedge at (1,1):
// base on the south edge, high edge to the north.
fn ramp_map() -> MapConfig {
    let mut cells = CellGrid::new(3, 3);
    for row in &mut cells.rows {
        for cell in row {
            cell.has_floor = true;
        }
    }
    let center = &mut cells.rows[1][1];
    center.has_ramp = true;
    center.ramp_base_south = true;
    center.ramp_top_north = true;
    MapConfig::for_grid(vec![level(cells, EdgeGrid::new(3, 3))], geometry(3, 3))
}

fn ramp_cell_center(nav_cols: i32) -> Position {
    let geometry = geometry(nav_cols, nav_cols);
    Position {
        x: geometry.cell_center_x(1),
        y: 0.0,
        z: geometry.cell_center_z(1),
    }
}

#[test]
fn side_entry_onto_ramp_is_routed_around() {
    let nav = NavGraph::new(ramp_map());
    // Start west of the wedge, zone east of it: the lateral crossing is
    // blocked, so the path must detour through row 0 or row 2.
    let start = Position {
        x: -4.0,
        y: 0.0,
        z: 0.0,
    };
    let path = nav
        .path_to_spawn_zone(&start, &zone(0, 2, 1))
        .expect("east cell should be reachable around the wedge");
    let ramp_center = ramp_cell_center(3);
    assert!(
        path.iter()
            .all(|p| (p.x - ramp_center.x).abs() > 0.1 || (p.z - ramp_center.z).abs() > 0.1),
        "path must not cross the wedge laterally: {path:?}"
    );
    assert!(path.len() > 1, "detour must be longer than the direct crossing");
}

#[test]
fn high_edge_entry_at_lower_level_is_blocked() {
    let nav = NavGraph::new(ramp_map());
    // Start north of the wedge (its high edge), zone = the ramp cell:
    // entry must detour around to the base on the south side.
    let start = Position {
        x: 0.0,
        y: 0.0,
        z: -4.0,
    };
    let path = nav
        .path_to_spawn_zone(&start, &zone(0, 1, 1))
        .expect("ramp cell should be reachable via its base");
    assert!(
        path.len() > 2,
        "high-edge entry must be rejected in favour of the base detour: {path:?}"
    );
}

#[test]
fn base_entry_onto_ramp_is_allowed() {
    let nav = NavGraph::new(ramp_map());
    // Start south of the wedge, right at its base edge: direct entry.
    let start = Position { x: 0.0, y: 0.0, z: 4.0 };
    let path = nav
        .path_to_spawn_zone(&start, &zone(0, 1, 1))
        .expect("base entry should path directly onto the ramp");
    assert_eq!(path.len(), 1, "base approach needs no detour: {path:?}");
}

#[test]
fn ramp_node_is_not_a_cover_destination() {
    let nav = NavGraph::new(ramp_map());
    let ramp = ramp_cell_center(3);
    let ramp_node = nav.node_for_position(&ramp).expect("ramp nav node");

    assert!(!nav.is_cover_destination(ramp_node));
}

// End-to-end guard on the real map: every actor zone must be reachable
// from every other zone's floor. Catches nav-rule regressions that
// disconnect levels (e.g. over-strict ramp gating) before they surface
// as in-game "NO nav path" spam.
#[test]
fn shipping_map_zones_are_mutually_reachable() {
    let server_gameplay_config =
        crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let map_name = &server_gameplay_config.default_map;
    let settings = &server_gameplay_config
        .maps
        .get(map_name)
        .expect("default map settings missing")
        .settings;
    let (barrier_kinds, bridge_kinds) = settings.kind_tables().expect("default map kind tables rejected");
    let GeneratedMap { config: map_config, .. } = crate::map::generate_map(
        map_name,
        settings.geometry,
        &|nested| server_gameplay_config.maps.get(nested).map(|map| map.settings.geometry),
        &barrier_kinds,
        &bridge_kinds,
    )
    .expect("default map failed to generate");
    let geometry = map_config.root_grid().geometry;
    let zones = map_config.actor_spawn_zones.clone();
    let nav = NavGraph::new(map_config);

    for (from_idx, from) in zones.iter().enumerate() {
        let (col, row) = from.cells().next().expect("zone rect is empty");
        let start = Position {
            x: geometry.cell_center_x(col),
            y: geometry.level_y(from.level),
            z: geometry.cell_center_z(row),
        };
        for (to_idx, to) in zones.iter().enumerate() {
            assert!(
                nav.path_to_spawn_zone(&start, to).is_some(),
                "no nav path from zone {from_idx} (level {}) to zone {to_idx} (level {})",
                from.level,
                to.level
            );
        }
    }
}

#[test]
fn floorless_arrival_strip_is_reachable() {
    // Like `path_uses_ramp_top_to_change_levels`, but the upper cell has
    // NO authored floor — only the opening. It sits above the slope's top
    // cell (the arrival strip), so it must still be standable, or actors
    // could never climb ramps whose exits aren't explicitly floored.
    let mut lower_cells = CellGrid::new(1, 2);
    lower_cells.rows[0][0].has_ramp = true;
    lower_cells.rows[0][0].ramp_base_north = true;
    lower_cells.rows[1][0].has_ramp = true;
    lower_cells.rows[1][0].ramp_top_south = true;
    let mut upper_cells = CellGrid::new(1, 2);
    upper_cells.rows[1][0].has_ramp_from_below = true;

    let nav = NavGraph::new(MapConfig::for_grid(
        vec![
            level(lower_cells, EdgeGrid::new(1, 2)),
            level(upper_cells, EdgeGrid::new(1, 2)),
        ],
        geometry(1, 2),
    ));

    let start = Position {
        x: 0.0,
        y: 0.0,
        z: -2.0,
    };
    assert!(
        nav.path_to_spawn_zone(&start, &zone(1, 0, 1)).is_some(),
        "the arrival strip above a ramp top must stay reachable without floor"
    );
}

#[test]
fn hole_over_ramp_base_is_not_traversable() {
    // The opening above the slope's BASE cell is a level-deep hole.
    let mut lower_cells = CellGrid::new(1, 2);
    lower_cells.rows[0][0].has_ramp = true;
    lower_cells.rows[0][0].ramp_base_north = true;
    lower_cells.rows[1][0].has_ramp = true;
    lower_cells.rows[1][0].ramp_top_south = true;
    let mut upper_cells = CellGrid::new(1, 2);
    upper_cells.rows[0][0].has_ramp_from_below = true;
    upper_cells.rows[1][0].has_ramp_from_below = true;

    let nav = NavGraph::new(MapConfig::for_grid(
        vec![
            level(lower_cells, EdgeGrid::new(1, 2)),
            level(upper_cells, EdgeGrid::new(1, 2)),
        ],
        geometry(1, 2),
    ));

    let start = Position {
        x: 0.0,
        y: 0.0,
        z: -2.0,
    };
    assert!(
        nav.path_to_spawn_zone(&start, &zone(1, 0, 0)).is_none(),
        "the opening above a ramp base is a hole, not a target"
    );
}

#[test]
fn arrival_strip_connects_only_through_the_top_side() {
    // 3×3 lower level with a ramp in col 1 (rows 1-2, top at row 1);
    // upper level has a bare strip at (1,1) and floor at (1,0), (0,0),
    // (0,1). The strip's only walkable side is north (the slope's top
    // direction), so a path to (0,1) must go via (1,0) and (0,0), never
    // strip→(0,1) directly across the slope's side ledge.
    let mut lower_cells = CellGrid::new(3, 3);
    for row in &mut lower_cells.rows {
        for cell in row {
            cell.has_floor = true;
        }
    }
    lower_cells.rows[1][1].has_ramp = true;
    lower_cells.rows[1][1].ramp_top_north = true;
    lower_cells.rows[2][1].has_ramp = true;
    lower_cells.rows[2][1].ramp_base_south = true;
    let mut upper_cells = CellGrid::new(3, 3);
    upper_cells.rows[1][1].has_ramp_from_below = true;
    upper_cells.rows[0][1].has_floor = true;
    upper_cells.rows[0][0].has_floor = true;
    upper_cells.rows[1][0].has_floor = true;

    let nav = NavGraph::new(MapConfig::for_grid(
        vec![
            level(lower_cells, EdgeGrid::new(3, 3)),
            level(upper_cells, EdgeGrid::new(3, 3)),
        ],
        geometry(3, 3),
    ));

    // Start on the ramp base cell (col 1, row 2).
    let start = Position { x: 0.0, y: 0.0, z: 4.0 };
    let path = nav
        .path_to_spawn_zone(&start, &zone(1, 0, 1))
        .expect("upper west cell should be reachable via the strip's top side");
    let geometry = geometry(3, 3);
    let via_top = Position {
        x: geometry.cell_center_x(1),
        y: LEVEL_HEIGHT,
        z: geometry.cell_center_z(0),
    };
    assert!(
        path.iter()
            .any(|p| (p.x - via_top.x).abs() < 0.1 && (p.z - via_top.z).abs() < 0.1 && (p.y - via_top.y).abs() < 0.1),
        "path must detour through the cell north of the strip: {path:?}"
    );
}

#[test]
fn path_uses_ramp_top_to_change_levels() {
    let mut lower_cells = CellGrid::new(1, 2);
    lower_cells.rows[0][0].has_floor = true;
    lower_cells.rows[0][0].has_ramp = true;
    lower_cells.rows[0][0].ramp_base_north = true;
    lower_cells.rows[1][0].has_floor = true;
    lower_cells.rows[1][0].has_ramp = true;
    lower_cells.rows[1][0].ramp_top_south = true;

    let mut upper_cells = CellGrid::new(1, 2);
    upper_cells.rows[1][0].has_floor = true;
    upper_cells.rows[1][0].has_ramp_from_below = true;

    let nav = NavGraph::new(MapConfig::for_grid(
        vec![
            level(lower_cells, EdgeGrid::new(1, 2)),
            level(upper_cells, EdgeGrid::new(1, 2)),
        ],
        geometry(1, 2),
    ));

    let start = Position {
        x: 0.0,
        y: 0.0,
        z: -2.0,
    };
    let path = nav
        .path_to_spawn_zone(&start, &zone(1, 0, 1))
        .expect("upper ramp top should be reachable");

    assert!(path.iter().any(|pos| (pos.y - LEVEL_HEIGHT).abs() < 0.001));

    let target = Position {
        y: LEVEL_HEIGHT,
        ..cell_center(1, 2, 0, 1)
    };
    let engagement = nav
        .engagement_route(&start, &target, 0.5, 0.5)
        .expect("engagement route should preserve the ramp transition");
    assert!(engagement.waypoints.len() > 1);
    assert!(engagement.waypoints.iter().any(|waypoint| waypoint.y < LEVEL_HEIGHT));
}

// Two cells side by side per row; a wall on the shared grid line covers
// row 0 only, so its end sits at the boundary between the rows.
fn wall_end_nav_and_world() -> (NavGraph, CollisionWorld) {
    let mut cells = CellGrid::new(2, 2);
    for row in &mut cells.rows {
        for cell in row {
            cell.has_floor = true;
        }
    }
    let mut edges = EdgeGrid::new(2, 2);
    edges.vertical[0][1] = true;
    let nav = NavGraph::new(MapConfig::for_grid(vec![level(cells, edges)], geometry(2, 2)));
    let world = CollisionWorld::from_map_layout(
        &MapLayout {
            walls: vec![Wall {
                x1: 0.0,
                z1: -CELL,
                x2: 0.0,
                z2: 0.0,
                width: WALL_THICKNESS,
                level: 0,
                y: 0.0,
                height: WALL_HEIGHT,
                carrier: CarrierId::WORLD,
            }],
            ..MapLayout::default()
        },
        &BarrierKindTable::default(),
    );
    (nav, world)
}

fn body(width: f32, depth: f32) -> common::config::CharacterPhysicsConfig {
    use common::config::{
        CharacterColliderAnchor, CharacterColliderConfig, CharacterPhysicsConfig, CharacterSupportProbeConfig,
    };
    CharacterPhysicsConfig {
        collider: CharacterColliderConfig {
            width,
            height: 1.0,
            depth,
            y_offset: 0.45,
            y_offset_anchor: CharacterColliderAnchor::Bottom,
        },
        support_probe: CharacterSupportProbeConfig { width: 0.2, depth: 0.2 },
    }
}

#[test]
fn route_start_detours_via_the_cell_centre_when_the_first_leg_clips_a_wall_end() {
    let (nav, world) = wall_end_nav_and_world();
    let start = Position {
        x: -0.8,
        y: 0.0,
        z: 1.0,
    };
    let target = cell_center(2, 2, 0, 0);
    let mut route = nav
        .engagement_route(&start, &target, 0.9, 0.7)
        .expect("the cell north of the start is adjacent");

    nav.anchor_route_start(&start, &mut route, &world, body(1.8, 1.4));

    assert_eq!(route.waypoints.len(), 2);
    assert_eq!(route.waypoints.front(), Some(&cell_center(2, 2, 0, 1)));
    assert_eq!(route.waypoints.back(), Some(&target));
}

#[test]
fn route_start_stays_direct_when_the_body_fits_past_the_wall_end() {
    let (nav, world) = wall_end_nav_and_world();
    let start = Position {
        x: -0.8,
        y: 0.0,
        z: 1.0,
    };
    let target = cell_center(2, 2, 0, 0);
    let mut route = nav
        .engagement_route(&start, &target, 0.15, 0.15)
        .expect("the cell north of the start is adjacent");

    nav.anchor_route_start(&start, &mut route, &world, body(0.3, 0.3));

    assert_eq!(route.waypoints.len(), 1);
    assert_eq!(route.waypoints.front(), Some(&target));
}

// Regression for sentries parking at the mouth of the hotel's basement
// ramp trench: off-centre at the base, the leg into the one-cell-wide
// trench dragged the body through the trench wall's end, and every replan
// produced the same leg.
#[test]
fn shipping_map_sentry_recentres_before_entering_the_basement_ramp_trench() {
    let server_gameplay_config =
        crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let gameplay_config = server_gameplay_config.gameplay_config();
    let settings = &server_gameplay_config
        .maps
        .get("hotel")
        .expect("hotel settings missing")
        .settings;
    let (barrier_kinds, bridge_kinds) = settings.kind_tables().expect("hotel kind tables rejected");
    let GeneratedMap {
        layout,
        config: map_config,
        ..
    } = crate::map::generate_map("hotel", settings.geometry, &|_| None, &barrier_kinds, &bridge_kinds)
        .expect("hotel map failed to generate");
    let world = CollisionWorld::from_map_layout(&layout, &barrier_kinds);
    let geometry = map_config.root_grid().geometry;
    let nav = NavGraph::new(map_config);
    let sentry = gameplay_config.expect_actor("sentry").physics();
    let center = |level: u8, col: i32, row: i32| Position {
        x: geometry.cell_center_x(col),
        y: geometry.level_y(level),
        z: geometry.cell_center_z(row),
    };
    let base = center(0, 0, 15);
    let start = Position {
        x: base.x + 0.9,
        z: base.z - 0.8,
        ..base
    };
    let target = center(1, 0, 12);
    let mut route = nav
        .engagement_route(
            &start,
            &target,
            sentry.collider.width / 2.0,
            sentry.collider.depth / 2.0,
        )
        .expect("the lobby is reachable up the basement ramp");
    let trench_entry = route.waypoints.front().copied().expect("route has a first leg");
    assert!(world.character_sweep_hits_wall(&start, &trench_entry, sentry));

    nav.anchor_route_start(&start, &mut route, &world, sentry);

    assert_eq!(route.waypoints.front(), Some(&base));
    assert!(!world.character_sweep_hits_wall(&start, &base, sentry));
    assert!(!world.character_sweep_hits_wall(&base, &trench_entry, sentry));
}
