use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::Resource;
use common::{
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    map::MapGeometry,
    map::compute_player_level,
    protocol::Position,
};

use crate::map::{ActorSpawnZone, Cell, EdgeGrid, MapConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NavNode {
    level: u8,
    row: i32,
    col: i32,
}

#[derive(Clone, Resource)]
pub struct NavGraph {
    map_config: MapConfig,
    geometry: MapGeometry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellSide {
    North,
    South,
    West,
    East,
}

impl NavGraph {
    #[must_use]
    pub const fn new(map_config: MapConfig, geometry: MapGeometry) -> Self {
        Self { map_config, geometry }
    }

    #[must_use]
    pub fn path_to_spawn_zone(&self, start: &Position, zone: &ActorSpawnZone) -> Option<VecDeque<Position>> {
        let start_node = self.nearest_node_for_position(start)?;
        let targets: HashSet<NavNode> = zone
            .cells()
            .filter_map(|(col, row)| {
                let node = NavNode {
                    level: zone.level,
                    row,
                    col,
                };
                self.is_traversable(node).then_some(node)
            })
            .collect();
        if targets.is_empty() {
            return None;
        }

        let mut queue = VecDeque::from([start_node]);
        let mut came_from: HashMap<NavNode, Option<NavNode>> = HashMap::from([(start_node, None)]);
        let mut target = targets.contains(&start_node).then_some(start_node);

        while target.is_none() {
            let Some(node) = queue.pop_front() else {
                break;
            };
            for next in self.neighbors(node) {
                if came_from.contains_key(&next) {
                    continue;
                }
                came_from.insert(next, Some(node));
                if targets.contains(&next) {
                    target = Some(next);
                    break;
                }
                queue.push_back(next);
            }
        }

        let target = target?;
        let mut nodes = Vec::new();
        let mut cursor = target;
        while cursor != start_node {
            nodes.push(cursor);
            cursor = came_from.get(&cursor).copied().flatten()?;
        }
        nodes.reverse();

        Some(nodes.into_iter().map(|node| self.node_center(node)).collect())
    }

    fn nearest_node_for_position(&self, pos: &Position) -> Option<NavNode> {
        let level = compute_player_level(pos.y);
        let row = self.geometry.world_z_to_cell_row(pos.z);
        let col = self.geometry.world_x_to_cell_col(pos.x);
        let direct = NavNode { level, row, col };
        if self.is_traversable(direct) {
            return Some(direct);
        }

        self.all_traversable_nodes().min_by(|a, b| {
            let a_score = self.node_position_score(*a, pos, level);
            let b_score = self.node_position_score(*b, pos, level);
            a_score.total_cmp(&b_score)
        })
    }

    fn all_traversable_nodes(&self) -> impl Iterator<Item = NavNode> + '_ {
        self.map_config
            .levels
            .iter()
            .enumerate()
            .flat_map(move |(level_idx, level_grid)| {
                let level = u8::try_from(level_idx).unwrap_or(u8::MAX);
                level_grid
                    .cells
                    .rows
                    .iter()
                    .enumerate()
                    .flat_map(move |(row_idx, cells)| {
                        let row = i32::try_from(row_idx).unwrap_or(i32::MAX);
                        cells.iter().enumerate().filter_map(move |(col_idx, _)| {
                            let col = i32::try_from(col_idx).unwrap_or(i32::MAX);
                            let node = NavNode { level, row, col };
                            self.is_traversable(node).then_some(node)
                        })
                    })
            })
    }

    fn node_position_score(&self, node: NavNode, pos: &Position, preferred_level: u8) -> f32 {
        let center = self.node_center(node);
        let dx = center.x - pos.x;
        let dz = center.z - pos.z;
        let level_penalty = if node.level == preferred_level {
            0.0
        } else {
            1_000_000.0
        };
        dx.mul_add(dx, dz * dz) + level_penalty
    }

    fn neighbors(&self, node: NavNode) -> Vec<NavNode> {
        let mut out = Vec::with_capacity(6);
        self.push_same_level_neighbor(&mut out, node, -1, 0, CellSide::North);
        self.push_same_level_neighbor(&mut out, node, 1, 0, CellSide::South);
        self.push_same_level_neighbor(&mut out, node, 0, -1, CellSide::West);
        self.push_same_level_neighbor(&mut out, node, 0, 1, CellSide::East);
        self.push_ramp_transition_neighbors(&mut out, node);
        out
    }

    fn push_same_level_neighbor(&self, out: &mut Vec<NavNode>, node: NavNode, dr: i32, dc: i32, side: CellSide) {
        let next = NavNode {
            level: node.level,
            row: node.row + dr,
            col: node.col + dc,
        };
        if !self.is_traversable(next) || self.has_blocking_edge_on_side(node, side) {
            return;
        }
        let (Some(node_cell), Some(next_cell)) = (self.cell(node), self.cell(next)) else {
            return;
        };
        if !ramp_edge_walkable(node_cell, next_cell, side) {
            return;
        }
        // A bare ramp opening's floor exists only along the slope's top edge;
        // every other side is a ledge over the slope. Restrict its same-level
        // edges to that one side.
        if self.opening_walk_side(node).is_some_and(|required| side != required) {
            return;
        }
        if self
            .opening_walk_side(next)
            .is_some_and(|required| opposite(side) != required)
        {
            return;
        }
        out.push(next);
    }

    // The single walkable side of a bare ramp opening (`has_ramp_from_below`
    // with neither floor nor own ramp): where the slope below meets the upper
    // floor. `None` for anything that isn't a bare opening — or for an
    // opening over a non-top slope cell, which is a plain hole.
    fn opening_walk_side(&self, node: NavNode) -> Option<CellSide> {
        let cell = self.cell(node)?;
        if cell.has_floor || cell.has_ramp || !cell.has_ramp_from_below || node.level == 0 {
            return None;
        }
        let below = self.cell(NavNode {
            level: node.level - 1,
            ..node
        })?;
        if !below.has_ramp {
            return None;
        }
        if below.ramp_top_north {
            Some(CellSide::North)
        } else if below.ramp_top_south {
            Some(CellSide::South)
        } else if below.ramp_top_west {
            Some(CellSide::West)
        } else if below.ramp_top_east {
            Some(CellSide::East)
        } else {
            None
        }
    }

    fn push_ramp_transition_neighbors(&self, out: &mut Vec<NavNode>, node: NavNode) {
        let Some(cell) = self.cell(node) else {
            return;
        };
        if cell.has_ramp && cell_is_ramp_top(cell) {
            let upper = NavNode {
                level: node.level.saturating_add(1),
                ..node
            };
            // The upper cell is standable by construction here — it sits
            // above this top cell, i.e. it's the arrival strip (or has its
            // own floor).
            if self
                .cell(upper)
                .is_some_and(|upper_cell| upper_cell.has_ramp_from_below)
            {
                out.push(upper);
            }
        }
        if cell.has_ramp_from_below && node.level > 0 {
            let lower = NavNode {
                level: node.level - 1,
                ..node
            };
            if self
                .cell(lower)
                .is_some_and(|lower_cell| lower_cell.has_ramp && cell_is_ramp_top(lower_cell))
            {
                out.push(lower);
            }
        }
    }

    // Blocked by a wall or an impassable barrier on this side. The barrier-edge
    // grid holds only barriers actors can never pass (no pressure plate);
    // pressure-plate barriers are omitted upstream so nav routes through them.
    fn has_blocking_edge_on_side(&self, node: NavNode, side: CellSide) -> bool {
        let Some(level) = self.map_config.levels.get(usize::from(node.level)) else {
            return true;
        };
        has_edge_on_cell_side(&level.edges, node.row, node.col, side)
            || has_edge_on_cell_side(&level.barrier_edges, node.row, node.col, side)
    }

    fn is_traversable(&self, node: NavNode) -> bool {
        let Some(cell) = self.cell(node) else {
            return false;
        };
        // A bare ramp opening (`has_ramp_from_below` without authored floor)
        // is standable only on its arrival strip — directly above the
        // slope's top cell; the rest of the opening is a hole over the slope.
        cell.has_floor || cell.has_ramp || self.opening_walk_side(node).is_some()
    }

    fn cell(&self, node: NavNode) -> Option<&Cell> {
        let level = self.map_config.levels.get(usize::from(node.level))?;
        if node.row < 0 || node.col < 0 {
            return None;
        }
        level.cells.rows.get(node.row as usize)?.get(node.col as usize)
    }

    fn node_center(&self, node: NavNode) -> Position {
        Position {
            x: self.geometry.cell_to_world_x(node.col) + GRID_CELL_SIZE / 2.0,
            y: f32::from(node.level) * LEVEL_HEIGHT,
            z: self.geometry.cell_to_world_z(node.row) + GRID_CELL_SIZE / 2.0,
        }
    }
}

fn cell_is_ramp_top(cell: &Cell) -> bool {
    cell.ramp_top_north || cell.ramp_top_south || cell.ramp_top_west || cell.ramp_top_east
}

const fn opposite(side: CellSide) -> CellSide {
    match side {
        CellSide::North => CellSide::South,
        CellSide::South => CellSide::North,
        CellSide::West => CellSide::East,
        CellSide::East => CellSide::West,
    }
}

const fn ramp_top_on_side(cell: &Cell, side: CellSide) -> bool {
    match side {
        CellSide::North => cell.ramp_top_north,
        CellSide::South => cell.ramp_top_south,
        CellSide::West => cell.ramp_top_west,
        CellSide::East => cell.ramp_top_east,
    }
}

const fn ramp_base_on_side(cell: &Cell, side: CellSide) -> bool {
    match side {
        CellSide::North => cell.ramp_base_north,
        CellSide::South => cell.ramp_base_south,
        CellSide::West => cell.ramp_base_west,
        CellSide::East => cell.ramp_base_east,
    }
}

// A ramp is a solid wedge with no authored wall edges around it: at the
// lower level only its base edge is walkable. The side faces are vertical
// wedge walls and the high edge is an up-to-LEVEL_HEIGHT face over solid
// volume, so those crossings are physically blocked even though the edge
// grids are empty there. Two adjacent ramp cells are the same wedge's
// footprint (lateral or along-axis on the slope) or two bases meeting at
// floor level — both walkable. Known limitation: two side-by-side ramps
// with opposite directions would be misjudged; the editor doesn't author
// that shape.
fn ramp_edge_walkable(node: &Cell, next: &Cell, side: CellSide) -> bool {
    if !node.has_ramp && !next.has_ramp {
        return true;
    }
    if (node.has_ramp && ramp_top_on_side(node, side)) || (next.has_ramp && ramp_top_on_side(next, opposite(side))) {
        return false;
    }
    if node.has_ramp && next.has_ramp {
        return true;
    }
    if next.has_ramp {
        ramp_base_on_side(next, opposite(side))
    } else {
        ramp_base_on_side(node, side)
    }
}

fn has_edge_on_cell_side(edges: &EdgeGrid, row: i32, col: i32, side: CellSide) -> bool {
    match side {
        CellSide::North => edges.horizontal[row as usize][col as usize],
        CellSide::South => edges.horizontal[(row + 1) as usize][col as usize],
        CellSide::West => edges.vertical[row as usize][col as usize],
        CellSide::East => edges.vertical[row as usize][(col + 1) as usize],
    }
}

#[cfg(test)]
mod tests {
    use crate::map::{ActorSpawnZone, CellGrid, EdgeGrid, LevelGrid};

    use super::*;

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
            kind: "mine_1".into(),
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
        let gameplay_config =
            common::config::GameplayConfig::load_default().expect("default gameplay config should load");
        let server_gameplay_config =
            crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let kind_table = common::protocol::BarrierKindTable::from_ids(gameplay_config.barrier_kinds.clone())
            .expect("barrier kind table should build from the default gameplay config");
        let (_, map_config, geometry) = crate::map::generate_map(&kind_table, &server_gameplay_config.default_map);
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
            path.iter().any(|p| (p.x - via_top.x).abs() < 0.1
                && (p.z - via_top.z).abs() < 0.1
                && (p.y - via_top.y).abs() < 0.1),
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
}
