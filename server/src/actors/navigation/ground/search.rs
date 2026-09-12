use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, VecDeque},
};

use bevy::prelude::Vec3;
use common::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    physics::{CollisionWorld, position_has_floor_support},
    protocol::{BarrierId, CarrierId, Position},
};

use super::{NavGraphs, NavNode, NavWaypoint, PlannedRoute};
use rand::{Rng, RngExt};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct Node {
    carrier: CarrierId,
    cell: NavNode,
}

#[derive(Clone, Copy)]
struct Entry {
    node: Node,
    distance: f32,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.node.cmp(&self.node))
    }
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
    pub(crate) fn roam(
        &self,
        start: Position,
        allowed: impl Fn(Position) -> bool,
        rng: &mut impl Rng,
    ) -> Option<PlannedRoute> {
        let mut candidates: Vec<_> = self
            .graphs
            .iter()
            .flat_map(|(carrier, graph)| {
                graph
                    .all_traversable_nodes()
                    .map(move |cell| self.position(Node { carrier, cell }))
            })
            .filter(|pos| allowed(*pos))
            .collect();
        for _ in 0..candidates.len().min(8) {
            let center = candidates.swap_remove(rng.random_range(0..candidates.len()));
            if let Some(route) = self.route(
                start,
                |pos, size| {
                    let matches = (pos.y - center.y).abs() < 0.1
                        && (pos.x - center.x).abs() < size * 0.5
                        && (pos.z - center.z).abs() < size * 0.5;
                    matches.then_some(center)
                },
                &allowed,
                2048,
                None,
            ) {
                let mut route = route;
                if let Some(last) = route.waypoints.back_mut() {
                    let size = self.graphs.get(self.carrier).cell_size();
                    let inset = (size * 0.5 - self.physics.movement_collider.radius() - 0.1).max(0.0);
                    let pose = self.carriers.pose(self.carrier);
                    let mut end = pose.transform_position(&last.position);
                    end.x += rng.random_range(-inset..=inset);
                    end.z += rng.random_range(-inset..=inset);
                    if allowed(end)
                        && self
                            .world
                            .character_ground_route_clear(center, end, self.physics, self.open)
                    {
                        last.position = pose.inverse_transform_position(&end);
                    }
                }
                return Some(route);
            }
        }
        None
    }

    pub(crate) fn route(
        &self,
        start: Position,
        goal: impl Fn(Position, f32) -> Option<Position>,
        allowed: impl Fn(Position) -> bool,
        work: usize,
        fallback: Option<&dyn Fn(Position) -> f32>,
    ) -> Option<PlannedRoute> {
        let graph = self.graphs.get(self.carrier);
        let pose = self.carriers.pose(self.carrier);
        let start_node = Node {
            carrier: self.carrier,
            cell: graph.route_start_node(&pose.inverse_transform_position(&start), graph.ladder_links(self.kind))?,
        };
        let mut queue = BinaryHeap::from([Entry {
            node: start_node,
            distance: 0.0,
        }]);
        let mut parents = HashMap::from([(start_node, start_node)]);
        let mut distances = HashMap::from([(start_node, 0.0)]);
        let mut retreat = None;
        let mut score = f32::NEG_INFINITY;
        for _ in 0..work {
            let Some(entry) = queue.pop() else { break };
            if entry.distance > distances[&entry.node] {
                continue;
            }
            let node = entry.node;
            let pos = if node == start_node { start } else { self.position(node) };
            let graph = self.graphs.get(node.carrier);
            if let Some(end) = goal(pos, graph.cell_size())
                && allowed(end)
                && !self.world.character_overlaps_solid(&end, self.physics, self.open)
                && self
                    .world
                    .character_ground_route_clear(pos, end, self.physics, self.open)
            {
                return self.reconstruct(start_node, node, end, &parents);
            }
            let candidate_score = fallback.map_or(f32::NEG_INFINITY, |score| score(pos));
            if node != start_node && candidate_score > score {
                score = candidate_score;
                retreat = Some(node);
            }
            for next in self.neighbors(node) {
                let points = self.edge(node, next);
                let mut previous = pos;
                let mut distance = entry.distance;
                let mut clear = true;
                for point in points {
                    let steps = (previous.distance_sq(&point.position).sqrt()
                        / (self.physics.movement_collider.radius() * 0.5).max(0.05))
                    .ceil()
                    .max(1.0) as usize;
                    let inside = (1..=steps).all(|index| {
                        allowed(
                            Vec3::from(previous)
                                .lerp(point.position.into(), index as f32 / steps as f32)
                                .into(),
                        )
                    });
                    if !inside
                        || !self
                            .world
                            .character_ground_route_clear(previous, point.position, self.physics, self.open)
                    {
                        clear = false;
                        break;
                    }
                    distance += previous.distance_sq(&point.position).sqrt();
                    previous = point.position;
                }
                if !clear || distances.get(&next).is_some_and(|known| *known <= distance) {
                    continue;
                }
                parents.insert(next, node);
                distances.insert(next, distance);
                queue.push(Entry { node: next, distance });
            }
        }
        retreat.and_then(|node| self.reconstruct(start_node, node, self.position(node), &parents))
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
