use std::collections::{HashMap, VecDeque};

use common::{config::CharacterPhysicsConfig, map::CarrierPose, physics::CollisionWorld, protocol::Position};
use rand::{Rng, RngExt};

use super::{LadderLink, NavGraph, NavNode, NavWaypoint, WaypointKind};

// Body clearance added to the footprint when a straight leg is judged
// against the floor cells it crosses.
pub(super) const DIRECT_ROUTE_CLEARANCE_MARGIN: f32 = 0.1;

#[derive(Debug, Clone)]
pub(crate) struct PlannedRoute {
    pub(crate) waypoints: VecDeque<NavWaypoint>,
    pub(crate) destination_node: NavNode,
}

impl NavGraph {
    pub(crate) fn engagement_route(
        &self,
        ladders: &[LadderLink],
        start: &Position,
        target: &Position,
        actor_half_width: f32,
        actor_half_depth: f32,
    ) -> Option<PlannedRoute> {
        let start_node = self.route_start_node(start, ladders)?;
        let target_node = self.nearest_node_for_position(target)?;
        let mut route = self.route_between_nodes(ladders, start_node, target_node, |_| true)?;
        if route
            .waypoints
            .back()
            .is_none_or(|last| last.position.horizontal_distance_sq(target) > 0.01)
        {
            route.waypoints.push_back(NavWaypoint::walk(*target));
        } else if let Some(last) = route.waypoints.back_mut() {
            if last.is_walk() {
                last.position = *target;
            } else if last.position.distance_sq(target) > 0.01 {
                route.waypoints.push_back(NavWaypoint::walk(*target));
            }
        }
        self.shortcut_flat_waypoints(
            start,
            &mut route.waypoints,
            actor_half_width + DIRECT_ROUTE_CLEARANCE_MARGIN,
            actor_half_depth + DIRECT_ROUTE_CLEARANCE_MARGIN,
        );
        Some(route)
    }

    fn shortcut_flat_waypoints(
        &self,
        start: &Position,
        waypoints: &mut VecDeque<NavWaypoint>,
        half_width: f32,
        half_depth: f32,
    ) {
        let mut remaining = std::mem::take(waypoints);
        let mut shortcut = VecDeque::new();
        let mut anchor = *start;
        while !remaining.is_empty() {
            let next_index = (0..remaining.len())
                .rev()
                .find(|&index| {
                    remaining.iter().take(index + 1).all(|point| point.is_walk())
                        && self.flat_path_is_clear(&anchor, &remaining[index].position, half_width, half_depth)
                })
                .unwrap_or(0);
            let next = remaining[next_index];
            remaining.drain(..=next_index);
            shortcut.push_back(next);
            anchor = next.position;
        }
        *waypoints = shortcut;
    }

    // Node-centre legs are footprint-safe by construction, but the actor
    // starts wherever it stands inside its cell: a first leg cut from a cell
    // corner can drag the body through a wall end, and the motor parks
    // instead of scraping along it. Detour via the current cell's centre.
    // The sweep is the one world query here, so `pose` (the graph's
    // carrier) puts the leg where the colliders are.
    pub(crate) fn anchor_route_start(
        &self,
        ladders: &[LadderLink],
        start: &Position,
        route: &mut PlannedRoute,
        collision_world: &CollisionWorld,
        physics: CharacterPhysicsConfig,
        pose: CarrierPose,
    ) {
        if route
            .waypoints
            .front()
            .is_some_and(|first| matches!(first.kind, WaypointKind::Climb { .. }))
        {
            return;
        }
        if self.prepend_ladder_exit(start, route, ladders) {
            return;
        }
        let Some(first) = route.waypoints.front() else { return };
        if !collision_world.character_sweep_hits_wall(
            &pose.transform_position(start),
            &pose.transform_position(&first.position),
            physics,
        ) {
            return;
        }
        let Some(node) = self.route_start_node(start, ladders) else {
            return;
        };
        route.waypoints.push_front(NavWaypoint::walk(self.node_center(node)));
    }

    // One-leg route to a random adjacent cell — the shake-loose hop for a
    // stalled actor.
    pub(crate) fn random_neighbor_route(
        &self,
        ladders: &[LadderLink],
        start: &Position,
        allowed: impl Fn(Position) -> bool,
        rng: &mut impl Rng,
    ) -> Option<PlannedRoute> {
        let node = self.route_start_node(start, ladders)?;
        let neighbors: Vec<_> = self
            .route_neighbors(node, ladders)
            .into_iter()
            .filter(|next| {
                self.edge_waypoints(node, *next, ladders)
                    .iter()
                    .all(|point| allowed(point.position))
            })
            .collect();
        if neighbors.is_empty() {
            return None;
        }
        let target = neighbors[rng.random_range(0..neighbors.len())];
        Some(PlannedRoute {
            waypoints: self.edge_waypoints(node, target, ladders).into(),
            destination_node: target,
        })
    }

    fn route_between_nodes(
        &self,
        ladders: &[LadderLink],
        start: NavNode,
        target: NavNode,
        allowed: impl Fn(NavNode) -> bool,
    ) -> Option<PlannedRoute> {
        if start == target {
            return Some(PlannedRoute {
                waypoints: VecDeque::new(),
                destination_node: target,
            });
        }
        self.route_to_any(ladders, start, |node| node == target, allowed)
    }

    pub(super) fn route_to_any(
        &self,
        ladders: &[LadderLink],
        start: NavNode,
        is_target: impl FnMut(NavNode) -> bool,
        allowed: impl Fn(NavNode) -> bool,
    ) -> Option<PlannedRoute> {
        // This structural traversal uses prevalidated edges and performs no physics queries.
        self.route_to_any_with_limit(ladders, start, is_target, allowed, usize::MAX)
    }

    fn route_to_any_with_limit(
        &self,
        ladders: &[LadderLink],
        start: NavNode,
        is_target: impl FnMut(NavNode) -> bool,
        allowed: impl Fn(NavNode) -> bool,
        max_depth: usize,
    ) -> Option<PlannedRoute> {
        let search = self.search(ladders, start, allowed, max_depth, is_target);
        self.route_from_search(ladders, start, search.stopped_at?, &search.came_from)
    }

    // Breadth-first from `start` through `allowed` nodes, at most `max_depth`
    // steps out, until `stop` accepts a dequeued node or the reach is exhausted.
    fn search(
        &self,
        ladders: &[LadderLink],
        start: NavNode,
        allowed: impl Fn(NavNode) -> bool,
        max_depth: usize,
        mut stop: impl FnMut(NavNode) -> bool,
    ) -> RouteSearch {
        let mut queue = VecDeque::from([start]);
        let mut came_from = HashMap::from([(start, None)]);
        let mut depths = HashMap::from([(start, 0usize)]);
        let mut stopped_at = None;
        while let Some(node) = queue.pop_front() {
            if stop(node) {
                stopped_at = Some(node);
                break;
            }
            let depth = depths[&node];
            if depth >= max_depth {
                continue;
            }
            for next in self.route_neighbors(node, ladders) {
                if came_from.contains_key(&next) || !allowed(next) {
                    continue;
                }
                came_from.insert(next, Some(node));
                depths.insert(next, depth + 1);
                queue.push_back(next);
            }
        }
        RouteSearch { came_from, stopped_at }
    }

    fn route_from_search(
        &self,
        ladders: &[LadderLink],
        start: NavNode,
        target: NavNode,
        came_from: &HashMap<NavNode, Option<NavNode>>,
    ) -> Option<PlannedRoute> {
        let mut nodes = VecDeque::new();
        let mut cursor = target;
        while cursor != start {
            nodes.push_front(cursor);
            cursor = came_from.get(&cursor).copied().flatten()?;
        }
        let mut previous = start;
        let waypoints = nodes
            .into_iter()
            .flat_map(|node| {
                let steps = self.edge_waypoints(previous, node, ladders);
                previous = node;
                steps
            })
            .collect();
        Some(PlannedRoute {
            waypoints,
            destination_node: target,
        })
    }
}

struct RouteSearch {
    came_from: HashMap<NavNode, Option<NavNode>>,
    stopped_at: Option<NavNode>,
}
