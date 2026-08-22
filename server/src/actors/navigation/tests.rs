use common::constants::{GRID_CELL_SIZE, LEVEL_HEIGHT};
use common::map::MapGeometry;
use common::protocol::Position;

use super::NavGraph;
use crate::map::{ActorSpawnZone, CellGrid, EdgeGrid, LevelGrid, MapConfig};

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
    let nav = NavGraph::new(
        MapConfig {
            levels: vec![level(cells, edges)],
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: Vec::new(),
            placed_items: Vec::new(),
            pressure_plates: Vec::new(),
        },
        MapGeometry::new(2, 2),
    );

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
    let nav = NavGraph::new(
        MapConfig {
            levels: vec![LevelGrid {
                cells,
                edges: EdgeGrid::new(2, 2),
                barrier_edges,
            }],
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: Vec::new(),
            placed_items: Vec::new(),
            pressure_plates: Vec::new(),
        },
        MapGeometry::new(2, 2),
    );

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
    MapConfig {
        levels: vec![level(cells, EdgeGrid::new(3, 3))],
        actor_spawn_zones: Vec::new(),
        player_spawn_zones: Vec::new(),
        placed_items: Vec::new(),
        pressure_plates: Vec::new(),
    }
}

fn ramp_cell_center(nav_cols: i32) -> Position {
    let geometry = MapGeometry::new(nav_cols, nav_cols);
    Position {
        x: geometry.cell_to_world_x(1) + GRID_CELL_SIZE / 2.0,
        y: 0.0,
        z: geometry.cell_to_world_z(1) + GRID_CELL_SIZE / 2.0,
    }
}

#[test]
fn side_entry_onto_ramp_is_routed_around() {
    let nav = NavGraph::new(ramp_map(), MapGeometry::new(3, 3));
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
    let nav = NavGraph::new(ramp_map(), MapGeometry::new(3, 3));
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
    let nav = NavGraph::new(ramp_map(), MapGeometry::new(3, 3));
    // Start south of the wedge, right at its base edge: direct entry.
    let start = Position { x: 0.0, y: 0.0, z: 4.0 };
    let path = nav
        .path_to_spawn_zone(&start, &zone(0, 1, 1))
        .expect("base entry should path directly onto the ramp");
    assert_eq!(path.len(), 1, "base approach needs no detour: {path:?}");
}

// End-to-end guard on the real map: every actor zone must be reachable
// from every other zone's floor. Catches nav-rule regressions that
// disconnect levels (e.g. over-strict ramp gating) before they surface
// as in-game "NO nav path" spam.
#[test]
fn shipping_map_zones_are_mutually_reachable() {
    let gameplay_config = common::config::GameplayConfig::load_default().expect("default gameplay config should load");
    let server_gameplay_config =
        crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let kind_table = common::protocol::BarrierKindTable::from_ids(gameplay_config.barrier_kinds.clone())
        .expect("barrier kind table should build from the default gameplay config");
    let (_, map_config, geometry) =
        crate::map::generate_map(&kind_table, &server_gameplay_config.default_map).expect("generate default map");
    let zones = map_config.actor_spawn_zones.clone();
    let nav = NavGraph::new(map_config, geometry);

    for (from_idx, from) in zones.iter().enumerate() {
        let (col, row) = from.cells().next().expect("zone rect is empty");
        let start = Position {
            x: geometry.cell_to_world_x(col) + GRID_CELL_SIZE / 2.0,
            y: f32::from(from.level) * LEVEL_HEIGHT,
            z: geometry.cell_to_world_z(row) + GRID_CELL_SIZE / 2.0,
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

    let nav = NavGraph::new(
        MapConfig {
            levels: vec![
                level(lower_cells, EdgeGrid::new(1, 2)),
                level(upper_cells, EdgeGrid::new(1, 2)),
            ],
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: Vec::new(),
            placed_items: Vec::new(),
            pressure_plates: Vec::new(),
        },
        MapGeometry::new(1, 2),
    );

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

    let nav = NavGraph::new(
        MapConfig {
            levels: vec![
                level(lower_cells, EdgeGrid::new(1, 2)),
                level(upper_cells, EdgeGrid::new(1, 2)),
            ],
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: Vec::new(),
            placed_items: Vec::new(),
            pressure_plates: Vec::new(),
        },
        MapGeometry::new(1, 2),
    );

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

    let nav = NavGraph::new(
        MapConfig {
            levels: vec![
                level(lower_cells, EdgeGrid::new(3, 3)),
                level(upper_cells, EdgeGrid::new(3, 3)),
            ],
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: Vec::new(),
            placed_items: Vec::new(),
            pressure_plates: Vec::new(),
        },
        MapGeometry::new(3, 3),
    );

    // Start on the ramp base cell (col 1, row 2).
    let start = Position { x: 0.0, y: 0.0, z: 4.0 };
    let path = nav
        .path_to_spawn_zone(&start, &zone(1, 0, 1))
        .expect("upper west cell should be reachable via the strip's top side");
    let geometry = MapGeometry::new(3, 3);
    let via_top = Position {
        x: geometry.cell_to_world_x(1) + GRID_CELL_SIZE / 2.0,
        y: LEVEL_HEIGHT,
        z: geometry.cell_to_world_z(0) + GRID_CELL_SIZE / 2.0,
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

    let nav = NavGraph::new(
        MapConfig {
            levels: vec![
                level(lower_cells, EdgeGrid::new(1, 2)),
                level(upper_cells, EdgeGrid::new(1, 2)),
            ],
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: Vec::new(),
            placed_items: Vec::new(),
            pressure_plates: Vec::new(),
        },
        MapGeometry::new(1, 2),
    );

    let path = nav
        .path_to_spawn_zone(
            &Position {
                x: 0.0,
                y: 0.0,
                z: -2.0,
            },
            &zone(1, 0, 1),
        )
        .expect("upper ramp top should be reachable");

    assert!(path.iter().any(|pos| (pos.y - LEVEL_HEIGHT).abs() < 0.001));
}
