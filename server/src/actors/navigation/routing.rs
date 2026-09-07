use std::collections::{HashMap, VecDeque};

use common::{config::CharacterPhysicsConfig, map::CarrierPose, physics::CollisionWorld, protocol::Position};
use rand::{Rng, RngExt};

use super::{LadderLink, NavGraph, NavNode, NavWaypoint, WaypointKind, territory::ActorTerritory};

// Threat exclusion radii for cover search, in grid cells.
const PRIMARY_THREAT_EXCLUSION_CELLS: f32 = 1.5;
const FALLBACK_THREAT_EXCLUSION_CELLS: f32 = 0.75;
const MINIMUM_THREAT_EXCLUSION_CELLS: f32 = 0.25;
const DIRECT_ROUTE_CLEARANCE_MARGIN: f32 = 0.1;
pub(super) const COVER_SEARCH_MAX_STEPS: usize = 12;

#[derive(Debug, Clone)]
pub(crate) struct PlannedRoute {
    pub(crate) waypoints: VecDeque<NavWaypoint>,
    pub(crate) destination_node: NavNode,
}

impl NavGraph {
    #[must_use]
    pub(crate) fn position_in_roam_region(&self, pos: &Position, territory: &ActorTerritory) -> bool {
        self.node_for_position(pos)
            .is_some_and(|node| territory.roam.contains(&node))
    }

    pub(crate) fn roam_route(
        &self,
        ladders: &[LadderLink],
        start: &Position,
        territory: &ActorTerritory,
        rng: &mut impl Rng,
    ) -> Option<PlannedRoute> {
        let start_node = self.route_start_node(start, ladders)?;
        let candidates: Vec<_> = territory
            .roam_nodes
            .iter()
            .copied()
            .filter(|node| *node != start_node)
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let offset = rng.random_range(0..candidates.len());
        for target in candidates.iter().cycle().skip(offset).take(candidates.len()) {
            if let Some(route) =
                self.route_between_nodes(ladders, start_node, *target, |node| territory.roam.contains(&node))
            {
                return Some(route);
            }
        }
        None
    }

    pub(crate) fn return_route(
        &self,
        ladders: &[LadderLink],
        start: &Position,
        territory: &ActorTerritory,
    ) -> Option<PlannedRoute> {
        let start_node = self.route_start_node(start, ladders)?;
        self.route_to_any(ladders, start_node, |node| territory.roam.contains(&node), |_| true)
    }

    pub(crate) fn engagement_route(
        &self,
        ladders: &[LadderLink],
        start: &Position,
        target: &Position,
        actor_half_width: f32,
        actor_half_depth: f32,
    ) -> Option<PlannedRoute> {
        let start_node = self.route_start_node(start, ladders)?;
        let target_node = self.node_for_position(target)?;
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
        rng: &mut impl Rng,
    ) -> Option<PlannedRoute> {
        let node = self.route_start_node(start, ladders)?;
        let neighbors = self.route_neighbors(node, ladders);
        if neighbors.is_empty() {
            return None;
        }
        let target = neighbors[rng.random_range(0..neighbors.len())];
        Some(PlannedRoute {
            waypoints: self.edge_waypoints(node, target, ladders).into(),
            destination_node: target,
        })
    }

    pub(crate) fn safe_cover_route(
        &self,
        ladders: &[LadderLink],
        start: &Position,
        threats: &[Position],
        is_stable_cover: impl FnMut(&Position) -> bool + Copy,
    ) -> Option<PlannedRoute> {
        let start_node = self.route_start_node(start, ladders)?;
        for exclusion_cells in [
            PRIMARY_THREAT_EXCLUSION_CELLS,
            FALLBACK_THREAT_EXCLUSION_CELLS,
            MINIMUM_THREAT_EXCLUSION_CELLS,
        ] {
            let exclusion_radius = exclusion_cells * self.cell_size();
            if let Some(route) =
                self.nearest_cover_route(ladders, start_node, threats, exclusion_radius, is_stable_cover)
            {
                return Some(route);
            }
        }
        None
    }

    // With no cover in reach the actor runs: a random cell within the cover
    // search's reach that is farther from the nearest threat than it stands,
    // or any reachable cell when it is cornered. `enter_evade` re-rolls the
    // leg when it ends.
    pub(crate) fn flee_route(
        &self,
        ladders: &[LadderLink],
        start: &Position,
        threats: &[Position],
        rng: &mut impl Rng,
    ) -> Option<PlannedRoute> {
        let start_node = self.route_start_node(start, ladders)?;
        let search = self.reachable(ladders, start_node, |_| true, COVER_SEARCH_MAX_STEPS);
        let mut reachable: Vec<NavNode> = search
            .depths
            .keys()
            .copied()
            .filter(|node| *node != start_node && self.is_cover_destination(*node))
            .collect();
        reachable.sort_unstable();
        let here = minimum_threat_distance_sq(*start, threats);
        let away: Vec<NavNode> = reachable
            .iter()
            .copied()
            .filter(|node| minimum_threat_distance_sq(self.node_center(*node), threats) > here)
            .collect();
        let candidates = if away.is_empty() { &reachable } else { &away };
        if candidates.is_empty() {
            return None;
        }
        let target = candidates[rng.random_range(0..candidates.len())];
        self.route_from_search(ladders, start_node, target, &search.came_from)
    }

    fn nearest_cover_route(
        &self,
        ladders: &[LadderLink],
        start: NavNode,
        threats: &[Position],
        exclusion_radius: f32,
        mut is_stable_cover: impl FnMut(&Position) -> bool,
    ) -> Option<PlannedRoute> {
        let exclusion_sq = exclusion_radius * exclusion_radius;
        self.route_to_any_with_limit(
            ladders,
            start,
            |node| node != start && self.is_cover_destination(node) && is_stable_cover(&self.node_center(node)),
            |node| {
                node == start
                    || threats
                        .iter()
                        .all(|threat| self.node_center(node).horizontal_distance_sq(threat) >= exclusion_sq)
            },
            COVER_SEARCH_MAX_STEPS,
        )
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

    fn route_to_any(
        &self,
        ladders: &[LadderLink],
        start: NavNode,
        is_target: impl FnMut(NavNode) -> bool,
        allowed: impl Fn(NavNode) -> bool,
    ) -> Option<PlannedRoute> {
        self.route_to_any_with_limit(ladders, start, is_target, allowed, usize::MAX)
    }

    fn route_to_any_with_limit(
        &self,
        ladders: &[LadderLink],
        start: NavNode,
        mut is_target: impl FnMut(NavNode) -> bool,
        allowed: impl Fn(NavNode) -> bool,
        max_depth: usize,
    ) -> Option<PlannedRoute> {
        let mut queue = VecDeque::from([start]);
        let mut came_from = HashMap::from([(start, None)]);
        let mut depths = HashMap::from([(start, 0usize)]);
        while let Some(node) = queue.pop_front() {
            if is_target(node) {
                return self.route_from_search(ladders, start, node, &came_from);
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
        None
    }

    fn reachable(
        &self,
        ladders: &[LadderLink],
        start: NavNode,
        allowed: impl Fn(NavNode) -> bool,
        max_depth: usize,
    ) -> ReachableSearch {
        let mut queue = VecDeque::from([start]);
        let mut came_from = HashMap::from([(start, None)]);
        let mut depths = HashMap::from([(start, 0usize)]);
        while let Some(node) = queue.pop_front() {
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
        ReachableSearch { came_from, depths }
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

struct ReachableSearch {
    came_from: HashMap<NavNode, Option<NavNode>>,
    depths: HashMap<NavNode, usize>,
}

fn minimum_threat_distance_sq(pos: Position, threats: &[Position]) -> f32 {
    threats
        .iter()
        .map(|threat| pos.horizontal_distance_sq(threat))
        .min_by(f32::total_cmp)
        .unwrap_or(f32::INFINITY)
}
