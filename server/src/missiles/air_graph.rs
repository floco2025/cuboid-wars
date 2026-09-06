use std::collections::VecDeque;

use bevy::prelude::*;

use common::{
    map::{Carriers, MapGeometry},
    physics::CollisionWorld,
    protocol::{BarrierKindId, CarrierId},
};

use super::steering::sweep_clear;
use crate::{map::MapConfig, pathfind::bfs_path};

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

struct AirGrid {
    carrier: CarrierId,
    geometry: MapGeometry,
    layers: i32,
}

// Each carrier supplies air-volume centers in its own frame, including a sky
// layer. Collision sweeps alone decide connectivity, including between grids.
#[derive(Resource)]
pub struct AirGraph {
    grids: Vec<AirGrid>,
}

impl AirGraph {
    #[must_use]
    pub fn new(map_config: &MapConfig) -> Self {
        Self {
            grids: map_config
                .grids
                .iter()
                .map(|grid| AirGrid {
                    carrier: grid.carrier,
                    geometry: grid.geometry,
                    layers: i32::try_from(grid.levels.len()).expect("map level count exceeds i32") + 1,
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn path(
        &self,
        carriers: &Carriers,
        world: &CollisionWorld,
        open_kinds: &[BarrierKindId],
        from: Vec3,
        to: Vec3,
        radius: f32,
    ) -> Option<VecDeque<Vec3>> {
        let clear = |a: Vec3, b: Vec3| sweep_clear(world, open_kinds, a, b - a, radius);
        if clear(from, to) {
            return Some(VecDeque::from([to]));
        }
        if !clear(from, from) {
            return None;
        }
        let nodes = bfs_path(
            SearchNode::Origin,
            |node| match node {
                SearchNode::Origin => false,
                SearchNode::Air(node) => clear(self.node_center(carriers, *node), to),
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
        if path.back() != Some(&to) {
            path.push_back(to);
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
mod tests {
    use super::*;
    use crate::{
        map::{CellGrid, EdgeGrid, LevelGrid},
        test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS, geometry},
    };
    use common::{
        constants::MISSILE_RADIUS,
        protocol::{Barrier, BarrierKindTable, Carrier, Floor, MapLayout, Position, Wall},
    };

    fn map(cols: i32, rows: i32, levels: usize) -> MapConfig {
        MapConfig::for_grid(
            (0..levels)
                .map(|_| LevelGrid {
                    cells: CellGrid::new(cols, rows),
                    edges: EdgeGrid::new(cols, rows),
                    barrier_edges: EdgeGrid::new(cols, rows),
                })
                .collect(),
            geometry(cols, rows),
        )
    }

    fn wall(x1: f32, z1: f32, x2: f32, z2: f32) -> Wall {
        Wall {
            x1,
            z1,
            x2,
            z2,
            width: WALL_THICKNESS,
            y: 0.0,
            height: WALL_HEIGHT,
            level: 0,
            carrier: CarrierId::WORLD,
        }
    }

    fn floor(x1: f32, z1: f32, x2: f32, z2: f32, y: f32) -> Floor {
        Floor {
            x1,
            z1,
            x2,
            z2,
            y,
            thickness: FLOOR_THICKNESS,
            level: 1,
            carrier: CarrierId::WORLD,
        }
    }

    fn world(layout: &MapLayout) -> CollisionWorld {
        CollisionWorld::from_map_layout(layout, &BarrierKindTable::default())
    }

    fn assert_clear_path(world: &CollisionWorld, from: Vec3, to: Vec3, path: &VecDeque<Vec3>) {
        let mut previous = from;
        for point in path {
            assert!(
                sweep_clear(world, &[], previous, *point - previous, MISSILE_RADIUS),
                "blocked leg {previous} -> {point}"
            );
            previous = *point;
        }
        assert_eq!(previous, to);
    }

    #[test]
    fn air_path_descends_through_a_floor_opening() {
        let graph = AirGraph::new(&map(2, 1, 2));
        let layout = MapLayout {
            floors: vec![floor(-3.4, -1.7, 0.0, 1.7, LEVEL_HEIGHT)],
            ..default()
        };
        let world = world(&layout);
        let from = Vec3::new(-1.7, LEVEL_HEIGHT + 1.0, 0.0);
        let to = Vec3::new(-1.7, 1.0, 0.0);
        let path = graph
            .path(&Carriers::default(), &world, &[], from, to, MISSILE_RADIUS)
            .expect("route through floor opening missing");
        assert_clear_path(&world, from, to, &path);
        assert!(path.iter().any(|point| point.x > 0.0));
    }

    #[test]
    fn air_path_crests_over_an_open_topped_wall() {
        let graph = AirGraph::new(&map(2, 1, 1));
        let layout = MapLayout {
            walls: vec![wall(0.0, -2.0, 0.0, 2.0)],
            ..default()
        };
        let world = world(&layout);
        let from = Vec3::new(-1.7, 1.0, 0.0);
        let to = Vec3::new(1.7, 1.0, 0.0);
        let path = graph
            .path(&Carriers::default(), &world, &[], from, to, MISSILE_RADIUS)
            .expect("route over wall missing");
        assert_clear_path(&world, from, to, &path);
        assert!(path.iter().any(|point| point.y > WALL_HEIGHT));
    }

    #[test]
    fn air_path_fails_when_fully_roofed() {
        let graph = AirGraph::new(&map(2, 1, 2));
        let layout = MapLayout {
            walls: vec![wall(0.0, -2.0, 0.0, 2.0)],
            floors: vec![floor(-3.5, -2.0, 3.5, 2.0, LEVEL_HEIGHT)],
            ..default()
        };
        let world = world(&layout);
        assert!(
            graph
                .path(
                    &Carriers::default(),
                    &world,
                    &[],
                    Vec3::new(-1.7, 1.0, 0.0),
                    Vec3::new(1.7, 1.0, 0.0),
                    MISSILE_RADIUS
                )
                .is_none()
        );
    }

    #[test]
    fn routes_into_a_shifted_room_keep_clear_of_its_walls_floor_and_roof() {
        let mut map = map(7, 7, 3);
        let room_grid = geometry(3, 3);
        let mut room = self::map(3, 3, 2).grids.remove(0);
        room.carrier = CarrierId(1);
        map.grids.push(room);
        let graph = AirGraph::new(&map);
        let half = room_grid.width() / 2.0;
        let door_half = room_grid.cell_size() / 2.0;
        let walls = [
            wall(-half, -half, half, -half),
            wall(-half, half, half, half),
            wall(half, -half, half, half),
            wall(-half, -half, -half, -door_half),
            wall(-half, door_half, -half, half),
            wall(0.0, -half, 0.0, door_half),
        ]
        .map(|wall| Wall {
            carrier: CarrierId(1),
            ..wall
        });
        let layout = MapLayout {
            walls: walls.to_vec(),
            floors: vec![
                Floor {
                    carrier: CarrierId(1),
                    ..floor(-half, -half, half, half, 0.0)
                },
                Floor {
                    carrier: CarrierId(1),
                    ..floor(-half, -half, half, half, LEVEL_HEIGHT)
                },
            ],
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 1,
                from: Position::from(Vec3::new(-1.1, 0.0, -0.7)),
                to: Position::from(Vec3::new(1.2, 2.3, 1.0)),
                travel_ticks: 120,
                pause_ticks: 30,
                phase_ticks: 0,
            }],
            ..default()
        };
        let mut carriers = Carriers::from_layout(&layout);
        let mut world = world(&layout);
        for tick in [0, 30, 60, 90, 120, 180, 240] {
            carriers.advance(tick);
            world.set_carrier_poses(&carriers);
            let target = carriers.pose(CarrierId(1)).transform_point(Vec3::new(2.0, 1.0, 1.0));
            for offset in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
                let from = (target + offset * 8.0).with_y(1.5);
                let path = graph
                    .path(&carriers, &world, &[], from, target, MISSILE_RADIUS)
                    .expect("route through moving room's door missing");
                assert_clear_path(&world, from, target, &path);
            }
        }
    }

    #[test]
    fn a_gap_narrower_than_the_missile_diameter_is_not_a_route() {
        let graph = AirGraph::new(&map(2, 1, 1));
        let layout = MapLayout {
            walls: vec![wall(0.0, -2.0, 0.0, -0.2), wall(0.0, 0.2, 0.0, 2.0)],
            floors: vec![floor(-3.5, -2.0, 3.5, 2.0, LEVEL_HEIGHT)],
            ..default()
        };
        let world = world(&layout);
        let from = Vec3::new(-1.7, 1.0, 0.0);
        let to = Vec3::new(1.7, 1.0, 0.0);
        assert!(
            graph
                .path(&Carriers::default(), &world, &[], from, to, MISSILE_RADIUS)
                .is_none()
        );
        assert!(graph.path(&Carriers::default(), &world, &[], from, to, 0.1).is_some());
    }

    #[test]
    fn opened_barriers_allow_a_route_without_stale_grid_flags() {
        let graph = AirGraph::new(&map(2, 1, 1));
        let layout = MapLayout {
            barriers: vec![Barrier {
                x1: 0.0,
                z1: -2.0,
                x2: 0.0,
                z2: 2.0,
                width: 0.2,
                y: 0.0,
                height: WALL_HEIGHT,
                level: 0,
                levels: 1,
                carrier: CarrierId::WORLD,
                kind: BarrierKindId(0),
            }],
            floors: vec![floor(-3.5, -2.0, 3.5, 2.0, LEVEL_HEIGHT)],
            ..default()
        };
        let kinds = BarrierKindTable::from_ids(vec!["gate".into()]).expect("test barrier catalog invalid");
        let world = CollisionWorld::from_map_layout(&layout, &kinds);
        let from = Vec3::new(-1.7, 1.0, 0.0);
        let to = Vec3::new(1.7, 1.0, 0.0);
        assert!(
            graph
                .path(&Carriers::default(), &world, &[], from, to, MISSILE_RADIUS)
                .is_none()
        );
        assert_eq!(
            graph.path(
                &Carriers::default(),
                &world,
                &[BarrierKindId(0)],
                from,
                to,
                MISSILE_RADIUS
            ),
            Some(VecDeque::from([to]))
        );
    }
}
