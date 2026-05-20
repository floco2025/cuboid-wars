use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::Resource;
use common::{
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    map::compute_player_level,
    map_geometry::MapGeometry,
    protocol::Position,
};

use crate::resources::{ActorSpawnZone, Cell, EdgeGrid, MapConfig};

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

#[derive(Debug, Clone, Copy)]
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
                        cells.iter().enumerate().filter_map(move |(col_idx, cell)| {
                            let col = i32::try_from(col_idx).unwrap_or(i32::MAX);
                            let node = NavNode { level, row, col };
                            cell_is_traversable(cell).then_some(node)
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
        if !self.is_traversable(next) || self.has_wall_on_side(node, side) {
            return;
        }
        out.push(next);
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

    fn has_wall_on_side(&self, node: NavNode, side: CellSide) -> bool {
        let Some(edges) = self.edges(node.level) else {
            return true;
        };
        has_edge_on_cell_side(edges, node.row, node.col, side)
    }

    fn is_traversable(&self, node: NavNode) -> bool {
        self.cell(node).is_some_and(cell_is_traversable)
    }

    fn cell(&self, node: NavNode) -> Option<&Cell> {
        let level = self.map_config.levels.get(usize::from(node.level))?;
        if node.row < 0 || node.col < 0 {
            return None;
        }
        level.cells.rows.get(node.row as usize)?.get(node.col as usize)
    }

    fn edges(&self, level: u8) -> Option<&EdgeGrid> {
        self.map_config.levels.get(usize::from(level)).map(|level| &level.edges)
    }

    fn node_center(&self, node: NavNode) -> Position {
        Position {
            x: self.geometry.cell_to_world_x(node.col) + GRID_CELL_SIZE / 2.0,
            y: f32::from(node.level) * LEVEL_HEIGHT,
            z: self.geometry.cell_to_world_z(node.row) + GRID_CELL_SIZE / 2.0,
        }
    }
}

fn cell_is_traversable(cell: &Cell) -> bool {
    cell.has_floor || cell.has_ramp || cell.has_ramp_from_below
}

fn cell_is_ramp_top(cell: &Cell) -> bool {
    cell.ramp_top_north || cell.ramp_top_south || cell.ramp_top_west || cell.ramp_top_east
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
    use crate::resources::{ActorSpawnZone, CellGrid, EdgeGrid, LevelGrid};

    use super::*;

    fn level(cells: CellGrid, edges: EdgeGrid) -> LevelGrid {
        LevelGrid { cells, edges }
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
                cookie_spawn_zones: Vec::new(),
                key_spawn_zones: Vec::new(),
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
                cookie_spawn_zones: Vec::new(),
                key_spawn_zones: Vec::new(),
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
