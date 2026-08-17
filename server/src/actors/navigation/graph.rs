use std::collections::{HashSet, VecDeque};

use bevy::prelude::Resource;
use common::{
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    map::MapGeometry,
    map::level_for_y,
    protocol::Position,
};

use crate::map::{ActorSpawnZone, Cell, CellSide, MapConfig, has_edge_on_cell_side};
use crate::pathfind::bfs_path;

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

        let nodes = bfs_path(start_node, |node| targets.contains(node), |node| self.neighbors(node))?;
        Some(nodes.into_iter().map(|node| self.node_center(node)).collect())
    }

    fn nearest_node_for_position(&self, pos: &Position) -> Option<NavNode> {
        let level = level_for_y(pos.y);
        let row = self.geometry.cell_row_containing_z(pos.z);
        let col = self.geometry.cell_col_containing_x(pos.x);
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
