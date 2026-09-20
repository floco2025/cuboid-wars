#[cfg(test)]
use std::collections::VecDeque;

use bevy::prelude::*;

use common::{
    config::MapGeometryConfig,
    map::{Carriers, MapGeometry},
    protocol::{CarrierId, MissileAirGrid},
};

#[cfg(test)]
use super::search::{AirSearch, SearchBudget, SearchProgress};
#[cfg(test)]
use common::{physics::CollisionWorld, protocol::FieldId};

const ADJACENT: [(i32, i32, i32); 6] = [(0, -1, 0), (0, 1, 0), (0, 0, -1), (0, 0, 1), (-1, 0, 0), (1, 0, 0)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct AirNode {
    pub grid: usize,
    pub layer: i32,
    pub row: i32,
    pub col: i32,
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

    #[cfg(test)]
    pub fn path(
        &self,
        carriers: &Carriers,
        world: &CollisionWorld,
        open_fields: &[FieldId],
        from: Vec3,
        to: Vec3,
        radius: f32,
        fuse_distance: f32,
    ) -> Option<VecDeque<Vec3>> {
        let mut search = AirSearch::new(self, carriers, open_fields, from, to, radius, fuse_distance);
        loop {
            match search.advance(self, carriers, world, &mut SearchBudget::new(128)) {
                SearchProgress::Pending => {}
                SearchProgress::Found(path) => return Some(path),
                SearchProgress::Unreachable | SearchProgress::Limited => return None,
            }
        }
    }

    #[must_use]
    pub fn cell_size(&self) -> f32 {
        self.grids
            .first()
            .expect("air graph has no root grid")
            .geometry
            .cell_size()
    }

    pub(super) fn node_at(&self, carriers: &Carriers, grid: usize, pos: Vec3) -> AirNode {
        let source = &self.grids[grid];
        let local = carriers.pose(source.carrier).inverse_transform_point(pos);
        AirNode {
            grid,
            layer: (local.y / source.geometry.level_height()).floor() as i32,
            row: source.geometry.cell_row_containing_z(local.z),
            col: source.geometry.cell_col_containing_x(local.x),
        }
    }

    pub(super) fn node_center(&self, carriers: &Carriers, node: AirNode) -> Vec3 {
        let grid = &self.grids[node.grid];
        carriers.pose(grid.carrier).transform_point(Vec3::new(
            grid.geometry.cell_center_x(node.col),
            (node.layer as f32 + 0.5) * grid.geometry.level_height(),
            grid.geometry.cell_center_z(node.row),
        ))
    }

    fn in_bounds(&self, node: AirNode, min: IVec3, max: IVec3) -> bool {
        if node.grid == 0 {
            let cell = IVec3::new(node.col, node.layer, node.row);
            return cell.cmpge(min).all() && cell.cmple(max).all();
        }
        let grid = &self.grids[node.grid];
        (0..grid.layers).contains(&node.layer)
            && (0..grid.geometry.grid_rows).contains(&node.row)
            && (0..grid.geometry.grid_cols).contains(&node.col)
    }

    pub(super) fn endpoint_candidates(&self, carriers: &Carriers, pos: Vec3, min: IVec3, max: IVec3) -> Vec<AirNode> {
        let mut nodes = Vec::new();
        for (grid, source) in self.grids.iter().enumerate() {
            let mut nearest = self.node_at(carriers, grid, pos);
            if grid == 0 {
                nearest.layer = nearest.layer.clamp(min.y, max.y);
                nearest.row = nearest.row.clamp(min.z, max.z);
                nearest.col = nearest.col.clamp(min.x, max.x);
            } else {
                nearest.layer = nearest.layer.clamp(0, source.layers - 1);
                nearest.row = nearest.row.clamp(0, source.geometry.grid_rows - 1);
                nearest.col = nearest.col.clamp(0, source.geometry.grid_cols - 1);
            }
            for dl in -1..=1 {
                for dr in -1..=1 {
                    for dc in -1..=1 {
                        let node = AirNode {
                            layer: nearest.layer + dl,
                            row: nearest.row + dr,
                            col: nearest.col + dc,
                            ..nearest
                        };
                        if self.in_bounds(node, min, max) {
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

    pub(super) fn neighbors(&self, carriers: &Carriers, node: AirNode, min: IVec3, max: IVec3) -> Vec<AirNode> {
        let mut nodes = Vec::new();
        let center = self.node_center(carriers, node);
        for grid in 0..self.grids.len() {
            let nearest = if grid == node.grid {
                node
            } else {
                let mut nearest = self.node_at(carriers, grid, center);
                if grid != 0 {
                    let source = &self.grids[grid];
                    nearest.col = nearest.col.clamp(0, source.geometry.grid_cols - 1);
                    nearest.row = nearest.row.clamp(0, source.geometry.grid_rows - 1);
                    nearest.layer = nearest.layer.clamp(0, source.layers - 1);
                    let reach = source.geometry.cell_size().max(source.geometry.level_height()) * 2.0;
                    if self.node_center(carriers, nearest).distance_squared(center) > reach * reach {
                        continue;
                    }
                }
                nearest
            };
            if grid != node.grid && self.in_bounds(nearest, min, max) {
                nodes.push(nearest);
            }
            for (dl, dr, dc) in ADJACENT {
                let next = AirNode {
                    layer: nearest.layer + dl,
                    row: nearest.row + dr,
                    col: nearest.col + dc,
                    ..nearest
                };
                if self.in_bounds(next, min, max) {
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
