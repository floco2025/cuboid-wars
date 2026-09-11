use std::collections::VecDeque;

use bevy::prelude::*;

use common::{
    config::MapGeometryConfig,
    map::{Carriers, MapGeometry},
    physics::CollisionWorld,
    protocol::{BarrierId, CarrierId, MissileAirGrid},
};

use super::{
    pathfind::bfs_path,
    steering::{sweep_clear, terminal_approach},
};

const ADJACENT: [(i32, i32, i32); 6] = [(0, -1, 0), (0, 1, 0), (0, 0, -1), (0, 0, 1), (-1, 0, 0), (1, 0, 0)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AirNode {
    grid: usize,
    layer: i32,
    row: i32,
    col: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SearchNode {
    Origin,
    Air(AirNode),
}

pub(super) struct AirGrid {
    pub(super) carrier: CarrierId,
    pub(super) geometry: MapGeometry,
    pub(super) layers: i32,
}

// Each carrier supplies air-volume centers in its own frame, including a sky
// layer. Collision sweeps alone decide connectivity, including between grids.
#[derive(Resource)]
pub struct AirGraph {
    pub(super) grids: Vec<AirGrid>,
}

impl AirGraph {
    #[must_use]
    pub fn new(grids: &[MissileAirGrid], sizes: MapGeometryConfig) -> Self {
        Self {
            grids: grids
                .iter()
                .map(|grid| AirGrid {
                    carrier: grid.carrier,
                    geometry: MapGeometry::new(grid.cols, grid.rows, sizes),
                    layers: i32::from(grid.levels) + 1,
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn path(
        &self,
        carriers: &Carriers,
        world: &CollisionWorld,
        open_kinds: &[BarrierId],
        from: Vec3,
        to: Vec3,
        radius: f32,
        fuse_distance: f32,
    ) -> Option<VecDeque<Vec3>> {
        let clear = |a: Vec3, b: Vec3| sweep_clear(world, open_kinds, a, b - a, radius);
        let approach = |from| terminal_approach(world, open_kinds, from, to, radius, fuse_distance);
        if let Some(end) = approach(from) {
            return Some(VecDeque::from([end]));
        }
        if !clear(from, from) {
            return None;
        }
        let nodes = bfs_path(
            SearchNode::Origin,
            |node| match node {
                SearchNode::Origin => false,
                SearchNode::Air(node) => approach(self.node_center(carriers, *node)).is_some(),
            },
            |node| {
                let (origin, candidates) = match node {
                    SearchNode::Origin => (from, self.endpoint_candidates(carriers, from)),
                    SearchNode::Air(node) => (self.node_center(carriers, node), self.neighbors(carriers, node)),
                };
                candidates
                    .into_iter()
                    .filter(|node| clear(origin, self.node_center(carriers, *node)))
                    .map(SearchNode::Air)
                    .collect()
            },
        )?;
        let mut path: VecDeque<_> = nodes
            .into_iter()
            .filter_map(|node| match node {
                SearchNode::Origin => None,
                SearchNode::Air(node) => Some(self.node_center(carriers, node)),
            })
            .collect();
        let end = approach(*path.back()?)?;
        if path.back() != Some(&end) {
            path.push_back(end);
        }
        Some(path)
    }

    #[must_use]
    pub fn cell_size(&self) -> f32 {
        self.grids
            .first()
            .expect("air graph has no root grid")
            .geometry
            .cell_size()
    }

    fn node_at(&self, carriers: &Carriers, grid: usize, pos: Vec3) -> AirNode {
        let source = &self.grids[grid];
        let local = carriers.pose(source.carrier).inverse_transform_point(pos);
        AirNode {
            grid,
            layer: (local.y / source.geometry.level_height()).floor() as i32,
            row: source.geometry.cell_row_containing_z(local.z),
            col: source.geometry.cell_col_containing_x(local.x),
        }
    }

    fn node_center(&self, carriers: &Carriers, node: AirNode) -> Vec3 {
        let grid = &self.grids[node.grid];
        carriers.pose(grid.carrier).transform_point(Vec3::new(
            grid.geometry.cell_center_x(node.col),
            (node.layer as f32 + 0.5) * grid.geometry.level_height(),
            grid.geometry.cell_center_z(node.row),
        ))
    }

    fn in_bounds(&self, node: AirNode) -> bool {
        let grid = &self.grids[node.grid];
        (0..grid.layers).contains(&node.layer)
            && (0..grid.geometry.grid_rows).contains(&node.row)
            && (0..grid.geometry.grid_cols).contains(&node.col)
    }

    fn endpoint_candidates(&self, carriers: &Carriers, pos: Vec3) -> Vec<AirNode> {
        let mut nodes = Vec::new();
        for (grid, source) in self.grids.iter().enumerate() {
            let mut nearest = self.node_at(carriers, grid, pos);
            nearest.layer = nearest.layer.clamp(0, source.layers - 1);
            nearest.row = nearest.row.clamp(0, source.geometry.grid_rows - 1);
            nearest.col = nearest.col.clamp(0, source.geometry.grid_cols - 1);
            for dl in -1..=1 {
                for dr in -1..=1 {
                    for dc in -1..=1 {
                        let node = AirNode {
                            layer: nearest.layer + dl,
                            row: nearest.row + dr,
                            col: nearest.col + dc,
                            ..nearest
                        };
                        if self.in_bounds(node) {
                            nodes.push(node);
                        }
                    }
                }
            }
        }
        nodes.sort_by(|a, b| {
            self.node_center(carriers, *a)
                .distance_squared(pos)
                .total_cmp(&self.node_center(carriers, *b).distance_squared(pos))
        });
        nodes
    }

    fn neighbors(&self, carriers: &Carriers, node: AirNode) -> Vec<AirNode> {
        let mut nodes = Vec::new();
        let center = self.node_center(carriers, node);
        for grid in 0..self.grids.len() {
            let nearest = if grid == node.grid {
                node
            } else {
                self.node_at(carriers, grid, center)
            };
            if grid != node.grid && self.in_bounds(nearest) {
                nodes.push(nearest);
            }
            for (dl, dr, dc) in ADJACENT {
                let next = AirNode {
                    layer: nearest.layer + dl,
                    row: nearest.row + dr,
                    col: nearest.col + dc,
                    ..nearest
                };
                if self.in_bounds(next) {
                    nodes.push(next);
                }
            }
        }
        nodes
    }
}

#[cfg(test)]
#[path = "tests/air_graph.rs"]
mod tests;
