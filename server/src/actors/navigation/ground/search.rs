use super::super::frontier::Frontier;
use std::collections::{BinaryHeap, HashMap, VecDeque};

use bevy::prelude::Vec3;
use common::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    physics::{CollisionWorld, position_has_floor_support},
    protocol::{BarrierId, CarrierId, Position},
};
use rand::{Rng, RngExt};

use super::{NavGraphs, NavNode, NavWaypoint, PlannedRoute};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct Node {
    carrier: CarrierId,
    cell: NavNode,
}

pub(crate) enum GroundSearchResult {
    Pending,
    Found(PlannedRoute),
    Unreachable,
}

pub(crate) struct GroundSearch {
    start: Position,
    start_node: Node,
    queue: BinaryHeap<Frontier<Node>>,
    parents: HashMap<Node, Node>,
    distances: HashMap<Node, f32>,
    retreat: Option<Node>,
    score: f32,
    expanded: usize,
}

pub(crate) struct GroundNavigation<'a> {
    pub graphs: &'a NavGraphs,
    pub carriers: &'a Carriers,
    pub carrier: CarrierId,
    pub kind: &'a str,
    pub world: &'a CollisionWorld,
    pub physics: CharacterPhysicsConfig,
    pub open: &'a [BarrierId],
}

impl GroundNavigation<'_> {
    pub(crate) fn search(&self, start: Position) -> Option<GroundSearch> {
        let graph = self.graphs.get(self.carrier);
        let pose = self.carriers.pose(self.carrier);
        let node = Node {
            carrier: self.carrier,
            cell: graph.route_start_node(&pose.inverse_transform_position(&start), graph.ladder_links(self.kind))?,
        };
        Some(GroundSearch {
            start,
            start_node: node,
            queue: BinaryHeap::from([Frontier {
                node,
                cost: 0.0,
                priority: 0.0,
                order: 0,
            }]),
            parents: HashMap::from([(node, node)]),
            distances: HashMap::from([(node, 0.0)]),
            retreat: None,
            score: f32::NEG_INFINITY,
            expanded: 0,
        })
    }

    pub(crate) fn advance(
        &self,
        search: &mut GroundSearch,
        goal: impl Fn(Position, f32) -> Option<Position>,
        allowed: impl Fn(Position, Position) -> bool,
        work: &mut usize,
        fallback: Option<&dyn Fn(Position) -> f32>,
        limit: Option<usize>,
    ) -> GroundSearchResult {
        while *work > 0 && limit.is_none_or(|limit| search.expanded < limit) {
            let Some(entry) = search.queue.pop() else {
                return self.finish(search);
            };
            *work -= 1;
            search.expanded += 1;
            if entry.cost > search.distances[&entry.node] {
                continue;
            }
            let node = entry.node;
            let graph = self.graphs.get(node.carrier);
            if !graph.is_traversable(node.cell) {
                continue;
            }
            let pos = if node == search.start_node {
                search.start
            } else {
                self.position(node)
            };
            if let Some(end) = goal(pos, graph.cell_size())
                && allowed(pos, end)
                && self
                    .world
                    .character_ground_route_clear(pos, end, self.physics, self.open)
                && let Some(route) = self.reconstruct(search.start_node, node, end, &search.parents)
            {
                return GroundSearchResult::Found(route);
            }
            let score = fallback.map_or(f32::NEG_INFINITY, |score| score(pos));
            if node != search.start_node && score > search.score {
                search.score = score;
                search.retreat = Some(node);
            }
            for next in self.neighbors(node) {
                let points = self.edge(node, next);
                let mut previous = pos;
                let mut distance = entry.cost;
                for point in &points {
                    distance += previous.distance_sq(&point.position).sqrt();
                    previous = point.position;
                }
                if search.distances.get(&next).is_some_and(|known| *known <= distance) {
                    continue;
                }
                let mut previous = pos;
                let clear = points.iter().all(|point| {
                    let inside = allowed(previous, point.position);
                    let clear = inside
                        && self
                            .world
                            .character_ground_route_clear(previous, point.position, self.physics, self.open);
                    previous = point.position;
                    clear
                });
                if !clear {
                    continue;
                }
                search.parents.insert(next, node);
                search.distances.insert(next, distance);
                search.queue.push(Frontier {
                    node: next,
                    cost: distance,
                    priority: distance,
                    order: search.distances.len(),
                });
            }
        }
        if limit.is_some_and(|limit| search.expanded >= limit) {
            self.finish(search)
        } else {
            GroundSearchResult::Pending
        }
    }

    fn finish(&self, search: &GroundSearch) -> GroundSearchResult {
        search
            .retreat
            .and_then(|node| self.reconstruct(search.start_node, node, self.position(node), &search.parents))
            .map_or(GroundSearchResult::Unreachable, GroundSearchResult::Found)
    }

    pub(crate) fn join_route(
        &self,
        start: Position,
        route: &mut PlannedRoute,
        allowed: &impl Fn(Position, Position) -> bool,
    ) -> bool {
        let pose = self.carriers.pose(self.carrier);
        let mut previous = start;
        let mut skip = 0;
        for (index, point) in route.waypoints.iter().enumerate() {
            if !point.is_walk() {
                break;
            }
            let end = pose.transform_position(&point.position);
            if start.distance_sq(&end) < self.graphs.get(self.carrier).cell_size().powi(2)
                && allowed(start, end)
                && self.walk_clear(start, end)
            {
                skip = index;
            }
        }
        route.waypoints.drain(..skip);
        route.waypoints.iter().all(|point| {
            let end = pose.transform_position(&point.position);
            let clear = allowed(previous, end)
                && self
                    .world
                    .character_ground_route_clear(previous, end, self.physics, self.open)
                && (!point.is_walk()
                    || !self
                        .graphs
                        .get(self.carrier)
                        .position_over_unpowered_bridge(&point.position));
            previous = end;
            clear
        })
    }

    fn walk_clear(&self, start: Position, end: Position) -> bool {
        let steps = (start.distance_sq(&end).sqrt() / self.physics.movement_collider.radius())
            .ceil()
            .max(1.0) as usize;
        self.world
            .character_ground_route_clear(start, end, self.physics, self.open)
            && (1..=steps).all(|index| {
                let position = Vec3::from(start).lerp(end.into(), index as f32 / steps as f32).into();
                position_has_floor_support(self.world, &position, self.physics)
            })
    }

    pub(crate) fn local_roam(
        &self,
        start: Position,
        allowed: impl Fn(Position) -> bool,
        work: &mut usize,
        rng: &mut impl Rng,
    ) -> Option<PlannedRoute> {
        let graph = self.graphs.get(self.carrier);
        let pose = self.carriers.pose(self.carrier);
        let local = pose.inverse_transform_position(&start);
        let node = graph.route_start_node(&local, graph.ladder_links(self.kind))?;
        if !graph.is_cover_destination(node) {
            return None;
        }
        let center = pose.transform_position(&graph.node_center(node));
        let inset = (graph.cell_size() * 0.5 - self.physics.movement_collider.radius() - 0.1).max(0.0);
        for _ in 0..8 {
            if *work == 0 {
                break;
            }
            *work -= 1;
            let end = Position {
                x: center.x + rng.random_range(-inset..=inset),
                z: center.z + rng.random_range(-inset..=inset),
                ..center
            };
            if allowed(end)
                && start.distance_sq(&end) > 0.04
                && self
                    .world
                    .character_ground_route_clear(start, end, self.physics, self.open)
            {
                return Some(PlannedRoute {
                    waypoints: VecDeque::from([NavWaypoint::walk(pose.inverse_transform_position(&end))]),
                    destination_node: node,
                });
            }
        }
        None
    }

    #[cfg(test)]
    pub(crate) fn route(
        &self,
        start: Position,
        goal: impl Fn(Position, f32) -> Option<Position>,
        allowed: impl Fn(Position, Position) -> bool,
        mut work: usize,
        fallback: Option<&dyn Fn(Position) -> f32>,
    ) -> Option<PlannedRoute> {
        let mut search = self.search(start)?;
        match self.advance(&mut search, goal, allowed, &mut work, fallback, None) {
            GroundSearchResult::Found(route) => Some(route),
            _ => None,
        }
    }

    fn position(&self, node: Node) -> Position {
        self.carriers
            .pose(node.carrier)
            .transform_position(&self.graphs.get(node.carrier).node_center(node.cell))
    }

    fn edge(&self, from: Node, to: Node) -> Vec<NavWaypoint> {
        if from.carrier != to.carrier {
            return vec![NavWaypoint::walk(self.position(to))];
        }
        let graph = self.graphs.get(from.carrier);
        let pose = self.carriers.pose(from.carrier);
        graph
            .edge_waypoints(from.cell, to.cell, graph.ladder_links(self.kind))
            .into_iter()
            .map(|mut point| {
                point.position = pose.transform_position(&point.position);
                point
            })
            .collect()
    }

    fn neighbors(&self, node: Node) -> Vec<Node> {
        let graph = self.graphs.get(node.carrier);
        let mut neighbors: Vec<_> = graph
            .route_neighbors(node.cell, graph.ladder_links(self.kind))
            .into_iter()
            .map(|cell| Node {
                carrier: node.carrier,
                cell,
            })
            .collect();
        if !graph.is_cover_destination(node.cell) {
            return neighbors;
        }
        let world_pos = self.position(node);
        for (carrier, other) in self.graphs.iter() {
            if carrier == node.carrier {
                continue;
            }
            let pose = self.carriers.pose(carrier);
            let local = pose.inverse_transform_position(&world_pos);
            let col = other.geometry.cell_col_containing_x(local.x);
            let row = other.geometry.cell_row_containing_z(local.z);
            let level = other.geometry.nearest_level_to_y(local.y);
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let cell = NavNode {
                        level,
                        row: row + dz,
                        col: col + dx,
                    };
                    if !other.is_traversable(cell) || !other.is_cover_destination(cell) {
                        continue;
                    }
                    let next = Node { carrier, cell };
                    let end = self.position(next);
                    let maximum = (graph.cell_size() + other.cell_size()) * 0.5 + 0.1;
                    if (end.y - world_pos.y).abs() > 0.1 || world_pos.distance_sq(&end) > maximum * maximum {
                        continue;
                    }
                    let steps = (maximum / (self.physics.movement_collider.radius() * 0.5).max(0.05)).ceil() as usize;
                    let clear = (0..=steps).all(|step| {
                        let point = Vec3::from(world_pos).lerp(end.into(), step as f32 / steps as f32);
                        position_has_floor_support(self.world, &point.into(), self.physics)
                    });
                    if clear
                        && self
                            .world
                            .character_flight_path_clear(world_pos, end, self.physics, self.open)
                    {
                        neighbors.push(next);
                    }
                }
            }
        }
        neighbors
    }

    fn reconstruct(
        &self,
        start: Node,
        target: Node,
        end: Position,
        parents: &HashMap<Node, Node>,
    ) -> Option<PlannedRoute> {
        let mut nodes = VecDeque::new();
        let mut cursor = target;
        while cursor != start {
            nodes.push_front(cursor);
            cursor = *parents.get(&cursor)?;
        }
        let pose = self.carriers.pose(start.carrier);
        let mut previous = start;
        let mut waypoints = VecDeque::new();
        let mut destination_node = start.cell;
        for node in nodes {
            for mut point in self.edge(previous, node) {
                point.position = pose.inverse_transform_position(&point.position);
                waypoints.push_back(point);
            }
            if node.carrier != start.carrier {
                return Some(PlannedRoute {
                    waypoints,
                    destination_node,
                });
            }
            destination_node = node.cell;
            previous = node;
        }
        let end = pose.inverse_transform_position(&end);
        if waypoints.back().is_none_or(|p| p.position.distance_sq(&end) > 0.0001) {
            waypoints.push_back(NavWaypoint::walk(end));
        }
        Some(PlannedRoute {
            waypoints,
            destination_node,
        })
    }
}
