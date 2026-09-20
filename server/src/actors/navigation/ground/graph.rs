use std::collections::HashMap;

use common::{
    constants::LEVEL_CLASSIFICATION_TOLERANCE,
    map::{Grounds, MapGeometry},
    protocol::{FieldId, Position},
};

use super::LadderLink;

use crate::map::{CarrierGrid, Cell, CellSide, LevelGrid, has_edge_on_cell_side};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct NavNode {
    pub(crate) level: u8,
    pub(crate) row: i32,
    pub(crate) col: i32,
}

// The walkable cells of one grid, in that grid's own frame: positions in
// and out are carrier-local, and the caller converts at the world boundary.
#[derive(Clone)]
pub struct NavGraph {
    pub(super) levels: Vec<LevelGrid>,
    pub(super) geometry: MapGeometry,
    // Links every bridge cell, so `open_fields` alone decides which of
    // them a route may use.
    adjacency: HashMap<NavNode, Vec<NavNode>>,
    open_fields: Vec<FieldId>,
    pub(super) ladder_routes: HashMap<String, Vec<LadderLink>>,
    // The exterior grounds, on the root graph of a map that has them. The
    // grid continues over them at their level as cells the graph never
    // stores — the terrain reaches the horizon — so their links are
    // computed on demand and their height read off the surface.
    grounds: Option<Grounds>,
}

impl NavGraph {
    // Build every potential bridge edge; switch state filters routes at runtime.
    #[must_use]
    pub fn new(grid: &CarrierGrid) -> Self {
        let mut graph = Self {
            levels: grid.levels.clone(),
            geometry: grid.geometry,
            adjacency: HashMap::new(),
            open_fields: Vec::new(),
            ladder_routes: HashMap::new(),
            grounds: None,
        };
        graph.adjacency = graph
            .all_traversable_nodes()
            .map(|node| (node, graph.calculate_neighbors(node)))
            .collect();
        graph
    }

    pub fn set_open_fields(&mut self, open: &[FieldId]) {
        self.open_fields = open.to_vec();
        self.open_fields.sort_unstable();
    }

    pub fn set_grounds(&mut self, grounds: Grounds) {
        self.grounds = Some(grounds);
    }

    fn outside_grid(&self, row: i32, col: i32) -> bool {
        !(0..self.geometry.grid_rows).contains(&row) || !(0..self.geometry.grid_cols).contains(&col)
    }

    // A cell of the grounds: outside the base footprint at the grounds'
    // level, with a cell to spare before the terrain ends.
    fn is_grounds(&self, node: NavNode) -> bool {
        let Some(grounds) = &self.grounds else {
            return false;
        };
        if node.level != grounds.settings.level {
            return false;
        }
        // Authored surfaces cut the footprint, so their cells skip the rectangle scan.
        let authored = self
            .cell(node)
            .is_some_and(|cell| cell.has_floor || cell.has_ramp || cell.ramp_below > 0 || cell.bridge.is_some());
        if authored {
            return false;
        }
        let x = self.geometry.cell_center_x(node.col);
        let z = self.geometry.cell_center_z(node.row);
        !grounds.is_inside_footprint(x, z)
            && grounds.distance_outside_bounds(x, z) <= grounds.extent() - self.geometry.cell_size()
    }

    fn bridge_open(&self, bridge: FieldId) -> bool {
        self.open_fields.binary_search(&bridge).is_ok()
    }

    // Whether `pos` stands over a bridge that is off.
    #[must_use]
    pub(crate) fn position_over_open_bridge(&self, pos: &Position) -> bool {
        self.cell(self.node_containing(pos))
            .and_then(|cell| cell.bridge)
            .is_some_and(|bridge| self.bridge_open(bridge))
    }

    #[must_use]
    pub(super) fn position_over_bridge(&self, pos: &Position) -> bool {
        self.cell(self.node_containing(pos))
            .is_some_and(|cell| cell.bridge.is_some())
    }

    #[must_use]
    pub(crate) fn contains(&self, pos: &Position) -> bool {
        let col = self.geometry.cell_col_containing_x(pos.x);
        let row = self.geometry.cell_row_containing_z(pos.z);
        let on_grounds = self.is_grounds(self.node_containing(pos));
        if self.outside_grid(row, col) {
            return on_grounds;
        }
        on_grounds
            || (pos.y >= -LEVEL_CLASSIFICATION_TOLERANCE
                && usize::from(self.geometry.level_for_y(pos.y)) < self.levels.len())
    }

    pub(super) fn neighbors(&self, node: NavNode) -> Vec<NavNode> {
        let mut out = self.adjacency.get(&node).cloned().unwrap_or_default();
        if self
            .grounds
            .as_ref()
            .is_some_and(|grounds| node.level == grounds.settings.level)
        {
            for (dr, dc, side) in [
                (-1, 0, CellSide::North),
                (1, 0, CellSide::South),
                (0, -1, CellSide::West),
                (0, 1, CellSide::East),
            ] {
                let next = NavNode {
                    row: node.row + dr,
                    col: node.col + dc,
                    ..node
                };
                if self.grounds_edge_walkable(node, next, side) {
                    out.push(next);
                }
            }
        }
        out
    }

    // Grounds links are implicit even inside the root grid. Only links
    // between authored cells are stored in adjacency.
    fn grounds_edge_walkable(&self, node: NavNode, next: NavNode, side: CellSide) -> bool {
        match (self.is_grounds(node), self.is_grounds(next)) {
            (true, true) => {
                !self.has_blocking_edge_on_side(node, side) && !self.has_blocking_edge_on_side(next, side.opposite())
            }
            (true, false) => self.rim_cell_open_toward_grounds(next, side.opposite()),
            (false, true) => self.rim_cell_open_toward_grounds(node, side),
            (false, false) => false,
        }
    }

    fn rim_cell_open_toward_grounds(&self, node: NavNode, side: CellSide) -> bool {
        self.is_traversable(node)
            && self.opening_walk_side(node).is_none()
            && !self.has_blocking_edge_on_side(node, side)
            && self
                .cell(node)
                .is_some_and(|cell| ramp_edge_walkable(cell, &Cell::default(), side))
    }

    pub(super) fn node_center(&self, node: NavNode) -> Position {
        if let Some(grounds) = &self.grounds
            && self.is_grounds(node)
        {
            let x = self.geometry.cell_center_x(node.col);
            let z = self.geometry.cell_center_z(node.row);
            return Position {
                x,
                y: grounds.height(x, z),
                z,
            };
        }
        let surface_node = self
            .opening_walk_side(node)
            .and_then(|_| self.ramp_node_under(node))
            .unwrap_or(node);
        Position {
            x: self.geometry.cell_center_x(node.col),
            y: self
                .cell(surface_node)
                .filter(|cell| cell.has_ramp)
                .map_or_else(|| self.geometry.level_y(node.level), |cell| cell.ramp_center_y),
            z: self.geometry.cell_center_z(node.row),
        }
    }

    #[must_use]
    pub(crate) fn cell_size(&self) -> f32 {
        self.geometry.cell_size()
    }

    pub(super) fn flat_path_is_clear(
        &self,
        start: &Position,
        target: &Position,
        half_width: f32,
        half_depth: f32,
    ) -> bool {
        let level = self.geometry.level_for_y(start.y);
        if self.geometry.level_for_y(target.y) != level {
            return false;
        }
        let dx = target.x - start.x;
        let dz = target.z - start.z;
        let distance = dx.hypot(dz);
        let steps = (distance / (self.geometry.cell_size() / 4.0)).ceil().max(1.0) as usize;
        let traces = [
            (0.0, 0.0),
            (-half_width, -half_depth),
            (-half_width, half_depth),
            (half_width, -half_depth),
            (half_width, half_depth),
        ];

        for (offset_x, offset_z) in traces {
            let mut previous = None;
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                let Some(node) =
                    self.flat_floor_node_at(start.x + dx * t + offset_x, start.z + dz * t + offset_z, level)
                else {
                    return false;
                };
                if previous.is_some_and(|previous| !self.flat_nodes_connect_directly(previous, node)) {
                    return false;
                }
                previous = Some(node);
            }
        }
        true
    }

    pub(super) fn is_cover_destination(&self, node: NavNode) -> bool {
        self.is_traversable(node)
            && self
                .cell(node)
                .is_none_or(|cell| !cell.has_ramp && cell.ramp_below == 0)
    }

    fn flat_floor_node_at(&self, x: f32, z: f32, level: u8) -> Option<NavNode> {
        let node = NavNode {
            level,
            row: self.geometry.cell_row_containing_z(z),
            col: self.geometry.cell_col_containing_x(x),
        };
        self.cell(node).is_some_and(Cell::is_flat_floor).then_some(node)
    }

    fn flat_nodes_connect_directly(&self, from: NavNode, to: NavNode) -> bool {
        let row_delta = to.row - from.row;
        let col_delta = to.col - from.col;
        if from.level != to.level || row_delta.abs() > 1 || col_delta.abs() > 1 {
            return false;
        }
        if row_delta == 0 || col_delta == 0 {
            return from == to || self.neighbors(from).contains(&to);
        }

        let row_first = NavNode {
            row: to.row,
            col: from.col,
            ..from
        };
        let col_first = NavNode {
            row: from.row,
            col: to.col,
            ..from
        };
        self.flat_edge_is_clear(from, row_first)
            && self.flat_edge_is_clear(row_first, to)
            && self.flat_edge_is_clear(from, col_first)
            && self.flat_edge_is_clear(col_first, to)
    }

    fn flat_edge_is_clear(&self, from: NavNode, to: NavNode) -> bool {
        self.cell(to).is_some_and(Cell::is_flat_floor) && self.neighbors(from).contains(&to)
    }

    // The node of the cell under `pos`, or the closest traversable one.
    #[must_use]
    pub(crate) fn nearest_node_for_position(&self, pos: &Position) -> Option<NavNode> {
        let direct = self.node_containing(pos);
        if self.is_traversable(direct) {
            return Some(direct);
        }

        self.all_traversable_nodes().min_by(|a, b| {
            let a_score = self.node_position_score(*a, pos, direct.level);
            let b_score = self.node_position_score(*b, pos, direct.level);
            a_score.total_cmp(&b_score)
        })
    }

    // Hills can lie inside the root grid. An authored surface at the
    // body's storey still wins, so an elevated course isn't mapped down to
    // the terrain underneath it. A slope's nodes all live on its lower storey,
    // while a body part-way up stands at the height of a storey above: with
    // nothing standable there, it is on the slope, so it resolves to the ramp's
    // node instead of the nearest floor beside the shaft.
    fn node_containing(&self, pos: &Position) -> NavNode {
        let row = self.geometry.cell_row_containing_z(pos.z);
        let col = self.geometry.cell_col_containing_x(pos.x);
        let node = NavNode {
            level: self.geometry.level_for_y(pos.y),
            row,
            col,
        };
        let standable = self
            .cell(node)
            .is_some_and(|cell| cell.has_floor || cell.has_ramp || cell.bridge.is_some())
            || self.opening_walk_side(node).is_some();
        if standable {
            return node;
        }
        if let Some(ramp_node) = self.ramp_node_under(node) {
            return ramp_node;
        }
        if let Some(grounds) = &self.grounds {
            let ground_node = NavNode {
                level: grounds.settings.level,
                ..node
            };
            if self.is_grounds(ground_node) {
                return ground_node;
            }
        }
        node
    }

    pub(super) fn all_traversable_nodes(&self) -> impl Iterator<Item = NavNode> + '_ {
        self.levels.iter().enumerate().flat_map(move |(level_idx, level_grid)| {
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

    pub(super) fn nodes_near(&self, pos: Position, radius: f32) -> impl Iterator<Item = NavNode> + '_ {
        let min_col = self.geometry.cell_col_containing_x(pos.x - radius);
        let max_col = self.geometry.cell_col_containing_x(pos.x + radius);
        let min_row = self.geometry.cell_row_containing_z(pos.z - radius);
        let max_row = self.geometry.cell_row_containing_z(pos.z + radius);
        let levels = self.levels.len().max(
            self.grounds
                .as_ref()
                .map_or(0, |grounds| usize::from(grounds.settings.level) + 1),
        );
        (0..levels).flat_map(move |level| {
            (min_row..=max_row).flat_map(move |row| {
                (min_col..=max_col).filter_map(move |col| {
                    let node = NavNode {
                        level: level as u8,
                        row,
                        col,
                    };
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

    fn calculate_neighbors(&self, node: NavNode) -> Vec<NavNode> {
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
            .is_some_and(|required| side.opposite() != required)
        {
            return;
        }
        out.push(next);
    }

    // The slope cell a ramp shaft cell lies over: `ramp_below` storeys down.
    fn ramp_node_under(&self, node: NavNode) -> Option<NavNode> {
        let below = self.cell(node)?.ramp_below;
        let ramp_node = NavNode {
            level: node.level.checked_sub(below).filter(|_| below > 0)?,
            ..node
        };
        self.cell(ramp_node)
            .is_some_and(|cell| cell.has_ramp)
            .then_some(ramp_node)
    }

    // The single walkable side of a bare ramp opening (a shaft cell with
    // neither floor nor own ramp, on the storey the slope arrives at): where
    // the slope meets the upper floor. `None` for anything that isn't a bare
    // opening: a storey the slope only passes, or an opening over a non-top
    // slope cell, which is a plain hole.
    fn opening_walk_side(&self, node: NavNode) -> Option<CellSide> {
        let cell = self.cell(node)?;
        if cell.has_floor || cell.has_ramp {
            return None;
        }
        let ramp_cell = self.cell(self.ramp_node_under(node)?)?;
        if ramp_cell.ramp_levels != cell.ramp_below {
            return None;
        }
        ramp_cell.ramp_top
    }

    fn push_ramp_transition_neighbors(&self, out: &mut Vec<NavNode>, node: NavNode) {
        let Some(cell) = self.cell(node) else {
            return;
        };
        if cell.has_ramp && cell.ramp_top.is_some() {
            let upper = NavNode {
                level: node.level.saturating_add(cell.ramp_levels),
                ..node
            };
            // The upper cell is standable by construction here — it sits
            // above this top cell, i.e. it's the arrival strip (or has its
            // own floor).
            if self
                .cell(upper)
                .is_some_and(|upper_cell| upper_cell.ramp_below == cell.ramp_levels)
            {
                out.push(upper);
            }
        }
        if let Some(lower) = self.ramp_node_under(node)
            && self
                .cell(lower)
                .is_some_and(|lower_cell| lower_cell.ramp_top.is_some() && lower_cell.ramp_levels == cell.ramp_below)
        {
            out.push(lower);
        }
    }

    // Blocked by a wall or an impassable barrier on this side. The barrier-edge
    // grid holds only barriers actors can never pass (on, with no pressure
    // plate to turn them off); the others are omitted upstream so nav routes
    // through them.
    fn has_blocking_edge_on_side(&self, node: NavNode, side: CellSide) -> bool {
        if self.outside_grid(node.row, node.col) {
            return false;
        }
        let Some(level) = self.levels.get(usize::from(node.level)) else {
            return true;
        };
        has_edge_on_cell_side(&level.edges, node.row, node.col, side)
            || has_edge_on_cell_side(&level.barrier_edges, node.row, node.col, side)
    }

    pub(super) fn is_traversable(&self, node: NavNode) -> bool {
        if self.is_grounds(node) {
            return true;
        }
        let Some(cell) = self.cell(node) else {
            return false;
        };
        if let Some(bridge) = cell.bridge {
            return !self.bridge_open(bridge);
        }
        // A bare ramp opening (a shaft cell without authored floor) is
        // standable only on its arrival strip — directly above the slope's
        // top cell; the rest of the opening is a hole over the slope.
        cell.has_floor || cell.has_ramp || self.opening_walk_side(node).is_some()
    }

    pub(super) fn cell(&self, node: NavNode) -> Option<&Cell> {
        let level = self.levels.get(usize::from(node.level))?;
        if node.row < 0 || node.col < 0 {
            return None;
        }
        level.cells.rows.get(node.row as usize)?.get(node.col as usize)
    }
}

// A ramp has no authored wall edges around it: at the lower level only its
// base edge is walkable. A wedge's side faces are vertical walls and its high
// edge is a face over solid volume, so those crossings are physically blocked
// even though the edge grids are empty there; past a plank's sides and high
// edge lies the space under the slope, where the graph has no node, so a
// plank is judged the same way. Two adjacent ramp cells are the same ramp's
// footprint (lateral or along-axis on the slope) or two bases meeting at
// floor level — both walkable. Known limitation: two side-by-side ramps
// with opposite directions would be misjudged.
fn ramp_edge_walkable(node: &Cell, next: &Cell, side: CellSide) -> bool {
    if !node.has_ramp && !next.has_ramp {
        return true;
    }
    if (node.has_ramp && node.ramp_top == Some(side)) || (next.has_ramp && next.ramp_top == Some(side.opposite())) {
        return false;
    }
    if node.has_ramp && next.has_ramp {
        return true;
    }
    if next.has_ramp {
        next.ramp_base == Some(side.opposite())
    } else {
        node.ramp_base == Some(side)
    }
}
