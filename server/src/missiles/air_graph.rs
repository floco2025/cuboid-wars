use std::collections::{HashSet, VecDeque};

use bevy::prelude::*;

use common::map::MapGeometry;

use crate::map::{Cell, CellSide, MapConfig, has_edge_on_cell_side};
use crate::pathfind::bfs_path;

// Adjacency offsets (layer, row, col) for the 6-connected air grid.
const ADJACENT: [(i32, i32, i32); 6] = [(0, -1, 0), (0, 1, 0), (0, 0, -1), (0, 0, 1), (-1, 0, 0), (1, 0, 0)];
// Cap on the nearest-open-volume fallback search.
const NEAREST_FLYABLE_MAX_VISITS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AirNode {
    layer: i32,
    row: i32,
    col: i32,
}

// Full-3D flyable-airspace graph for missiles: one node per storey-sized air
// volume (grid cell × level), plus one open sky layer above the top level.
// Horizontal neighbors connect where no wall/keyed-barrier edge separates
// them; vertical neighbors connect where the upper volume has no physical
// floor slab. Ramp cells are excluded — the wedge fills their air. Unlike
// the actors' `NavGraph`, standability is irrelevant: missiles fly, with no
// preferred direction.
#[derive(Resource)]
pub struct AirGraph {
    map_config: MapConfig,
    geometry: MapGeometry,
    layers: i32,
}

impl AirGraph {
    #[must_use]
    pub fn new(map_config: MapConfig, geometry: MapGeometry) -> Self {
        let layers = i32::try_from(map_config.levels.len()).unwrap_or(0) + 1;
        Self {
            map_config,
            geometry,
            layers,
        }
    }

    // BFS route from `from` to `to` as waypoints at air-volume centers.
    // `None` when no route through open air exists.
    #[must_use]
    pub fn path(&self, from: Vec3, to: Vec3) -> Option<VecDeque<Vec3>> {
        let start = self.nearest_flyable(self.node_at(from))?;
        let goal = self.nearest_flyable(self.node_at(to))?;

        let nodes = bfs_path(start, |node| *node == goal, |node| self.neighbors(node))?;
        Some(nodes.into_iter().map(|node| self.node_center(node)).collect())
    }

    #[must_use]
    pub fn cell_size(&self) -> f32 {
        self.geometry.cell_size()
    }

    fn node_at(&self, pos: Vec3) -> AirNode {
        let layer = (pos.y / self.geometry.level_height()).floor() as i32;
        AirNode {
            layer: layer.clamp(0, self.layers - 1),
            row: self.geometry.cell_row_containing_z(pos.z),
            col: self.geometry.cell_col_containing_x(pos.x),
        }
    }

    fn node_center(&self, node: AirNode) -> Vec3 {
        Vec3::new(
            self.geometry.cell_center_x(node.col),
            (node.layer as f32 + 0.5) * self.geometry.level_height(),
            self.geometry.cell_center_z(node.row),
        )
    }

    fn in_bounds(&self, node: AirNode) -> bool {
        (0..self.layers).contains(&node.layer)
            && (0..self.geometry.grid_rows).contains(&node.row)
            && (0..self.geometry.grid_cols).contains(&node.col)
    }

    fn flyable(&self, node: AirNode) -> bool {
        if !self.in_bounds(node) {
            return false;
        }
        self.cell(node).is_none_or(|cell| !cell.has_ramp)
    }

    // `None` above the top map level (open sky) or off-grid.
    fn cell(&self, node: AirNode) -> Option<&Cell> {
        let level = self.map_config.levels.get(usize::try_from(node.layer).ok()?)?;
        level
            .cells
            .rows
            .get(usize::try_from(node.row).ok()?)?
            .get(usize::try_from(node.col).ok()?)
    }

    // A clamped start/target can land in a ramp cell (a target standing on a
    // ramp): fall back to the nearest open volume.
    fn nearest_flyable(&self, node: AirNode) -> Option<AirNode> {
        if self.flyable(node) {
            return Some(node);
        }
        let mut queue = VecDeque::from([node]);
        let mut seen: HashSet<AirNode> = HashSet::from([node]);
        while let Some(current) = queue.pop_front() {
            if seen.len() > NEAREST_FLYABLE_MAX_VISITS {
                break;
            }
            for (dl, dr, dc) in ADJACENT {
                let next = AirNode {
                    layer: current.layer + dl,
                    row: current.row + dr,
                    col: current.col + dc,
                };
                if !self.in_bounds(next) || !seen.insert(next) {
                    continue;
                }
                if self.flyable(next) {
                    return Some(next);
                }
                queue.push_back(next);
            }
        }
        None
    }

    fn neighbors(&self, node: AirNode) -> Vec<AirNode> {
        let mut out = Vec::with_capacity(6);
        self.push_horizontal(&mut out, node, -1, 0, CellSide::North);
        self.push_horizontal(&mut out, node, 1, 0, CellSide::South);
        self.push_horizontal(&mut out, node, 0, -1, CellSide::West);
        self.push_horizontal(&mut out, node, 0, 1, CellSide::East);
        self.push_vertical(&mut out, node, 1);
        self.push_vertical(&mut out, node, -1);
        out
    }

    fn push_horizontal(&self, out: &mut Vec<AirNode>, node: AirNode, dr: i32, dc: i32, side: CellSide) {
        let next = AirNode {
            row: node.row + dr,
            col: node.col + dc,
            ..node
        };
        if !self.flyable(next) {
            return;
        }
        // The sky layer has no edge grids; map layers block on wall and
        // keyed-barrier edges (plate barriers are omitted upstream, so a
        // path may cross a closed plate and detonate there — accepted).
        if let Ok(layer) = usize::try_from(node.layer)
            && let Some(level) = self.map_config.levels.get(layer)
            && (has_edge_on_cell_side(&level.edges, node.row, node.col, side)
                || has_edge_on_cell_side(&level.barrier_edges, node.row, node.col, side))
        {
            return;
        }
        out.push(next);
    }

    fn push_vertical(&self, out: &mut Vec<AirNode>, node: AirNode, up: i32) {
        let next = AirNode {
            layer: node.layer + up,
            ..node
        };
        if !self.flyable(next) {
            return;
        }
        // The slab between two stacked volumes belongs to the upper one.
        let upper = if up > 0 { next } else { node };
        if self.cell(upper).is_some_and(|cell| cell.has_floor_slab) {
            return;
        }
        out.push(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        map::{CellGrid, EdgeGrid, LevelGrid},
        test_geometry::{LEVEL_HEIGHT, geometry},
    };

    fn level(cells: CellGrid, edges: EdgeGrid) -> LevelGrid {
        let rows = i32::try_from(cells.rows.len()).unwrap_or(0);
        let cols = i32::try_from(cells.rows.first().map_or(0, Vec::len)).unwrap_or(0);
        LevelGrid {
            cells,
            edges,
            barrier_edges: EdgeGrid::new(cols, rows),
        }
    }

    fn config(levels: Vec<LevelGrid>) -> MapConfig {
        MapConfig {
            levels,
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: Vec::new(),
            placed_items: Vec::new(),
            pressure_plates: Vec::new(),
        }
    }

    fn floored_cell() -> Cell {
        Cell {
            has_floor: true,
            has_floor_slab: true,
            ..Cell::default()
        }
    }

    #[test]
    fn air_path_descends_through_a_floor_opening() {
        // Two columns, two levels. The upper level has a slab over col 0 and
        // an opening at col 1: a missile above the slab must route sideways
        // and dive through the opening to reach the lower level.
        let mut lower = CellGrid::new(2, 1);
        lower.rows[0][0] = floored_cell();
        lower.rows[0][1] = floored_cell();
        let mut upper = CellGrid::new(2, 1);
        upper.rows[0][0] = floored_cell();

        let graph = AirGraph::new(
            config(vec![
                level(lower, EdgeGrid::new(2, 1)),
                level(upper, EdgeGrid::new(2, 1)),
            ]),
            geometry(2, 1),
        );
        let from = graph.node_center(AirNode {
            layer: 1,
            row: 0,
            col: 0,
        });
        let to = graph.node_center(AirNode {
            layer: 0,
            row: 0,
            col: 0,
        });

        let path = graph.path(from, to).expect("route through the opening should exist");

        assert!(
            path.iter().any(|wp| wp.y < LEVEL_HEIGHT),
            "path must descend into the lower level: {path:?}"
        );
        let last = path.back().expect("path is non-empty");
        assert!((*last - to).length() < 0.01, "path ends at the target volume");
    }

    #[test]
    fn air_path_crests_over_an_open_topped_wall() {
        // One level, a full-height wall between the two columns, open sky
        // above: the only route is up and over.
        let mut cells = CellGrid::new(2, 1);
        cells.rows[0][0] = floored_cell();
        cells.rows[0][1] = floored_cell();
        let mut edges = EdgeGrid::new(2, 1);
        edges.vertical[0][1] = true;

        let graph = AirGraph::new(config(vec![level(cells, edges)]), geometry(2, 1));
        let from = graph.node_center(AirNode {
            layer: 0,
            row: 0,
            col: 0,
        });
        let to = graph.node_center(AirNode {
            layer: 0,
            row: 0,
            col: 1,
        });

        let path = graph.path(from, to).expect("route over the wall should exist");

        assert!(
            path.iter().any(|wp| wp.y > LEVEL_HEIGHT),
            "path must climb over the wall: {path:?}"
        );
    }

    #[test]
    fn air_path_fails_when_fully_roofed() {
        // Same wall, but a solid roof over both columns: no route.
        let mut cells = CellGrid::new(2, 1);
        cells.rows[0][0] = floored_cell();
        cells.rows[0][1] = floored_cell();
        let mut edges = EdgeGrid::new(2, 1);
        edges.vertical[0][1] = true;
        let mut roof = CellGrid::new(2, 1);
        roof.rows[0][0].has_floor_slab = true;
        roof.rows[0][1].has_floor_slab = true;

        let graph = AirGraph::new(
            config(vec![level(cells, edges), level(roof, EdgeGrid::new(2, 1))]),
            geometry(2, 1),
        );
        let from = graph.node_center(AirNode {
            layer: 0,
            row: 0,
            col: 0,
        });
        let to = graph.node_center(AirNode {
            layer: 0,
            row: 0,
            col: 1,
        });

        assert!(graph.path(from, to).is_none(), "sealed rooms have no air route");
    }
}
