use std::collections::{HashMap, VecDeque};

use common::{config::CharacterPhysicsConfig, map::CarrierPose, physics::CollisionWorld, protocol::Position};
use rand::{Rng, RngExt};

use super::{NavGraph, NavNode, territory::ActorTerritory};

// Threat exclusion radii for cover search, in grid cells.
const PRIMARY_THREAT_EXCLUSION_CELLS: f32 = 1.5;
const FALLBACK_THREAT_EXCLUSION_CELLS: f32 = 0.75;
const MINIMUM_THREAT_EXCLUSION_CELLS: f32 = 0.25;
const DIRECT_ROUTE_CLEARANCE_MARGIN: f32 = 0.1;
pub(super) const COVER_SEARCH_MAX_STEPS: usize = 12;

#[derive(Debug, Clone)]
pub(crate) struct PlannedRoute {
    pub(crate) waypoints: VecDeque<Position>,
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
        start: &Position,
        territory: &ActorTerritory,
        rng: &mut impl Rng,
    ) -> Option<PlannedRoute> {
        let start_node = self.node_for_position(start)?;
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
            if let Some(route) = self.route_between_nodes(start_node, *target, |node| territory.roam.contains(&node)) {
                return Some(route);
            }
        }
        None
    }

    pub(crate) fn return_route(&self, start: &Position, territory: &ActorTerritory) -> Option<PlannedRoute> {
        let start_node = self.node_for_position(start)?;
        self.route_to_any(start_node, |node| territory.roam.contains(&node), |_| true)
    }

    pub(crate) fn engagement_route(
        &self,
        start: &Position,
        target: &Position,
        actor_half_width: f32,
        actor_half_depth: f32,
    ) -> Option<PlannedRoute> {
        let start_node = self.node_for_position(start)?;
        let target_node = self.node_for_position(target)?;
        let mut route = self.route_between_nodes(start_node, target_node, |_| true)?;
        if route
            .waypoints
            .back()
            .is_none_or(|last| last.horizontal_distance_sq(target) > 0.01)
        {
            route.waypoints.push_back(*target);
        } else if let Some(last) = route.waypoints.back_mut() {
            *last = *target;
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
        waypoints: &mut VecDeque<Position>,
        half_width: f32,
        half_depth: f32,
    ) {
        let mut remaining = std::mem::take(waypoints);
        let mut shortcut = VecDeque::new();
        let mut anchor = *start;
        while !remaining.is_empty() {
            let next_index = (0..remaining.len())
                .rev()
                .find(|&index| self.flat_path_is_clear(&anchor, &remaining[index], half_width, half_depth))
                .unwrap_or(0);
            let next = remaining[next_index];
            remaining.drain(..=next_index);
            shortcut.push_back(next);
            anchor = next;
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
        start: &Position,
        route: &mut PlannedRoute,
        collision_world: &CollisionWorld,
        physics: CharacterPhysicsConfig,
        pose: CarrierPose,
    ) {
        let Some(first) = route.waypoints.front() else {
            return;
        };
        if !collision_world.character_sweep_hits_wall(
            &pose.transform_position(start),
            &pose.transform_position(first),
            physics,
        ) {
            return;
        }
        let Some(node) = self.node_for_position(start) else {
            return;
        };
        route.waypoints.push_front(self.node_center(node));
    }

    // One-leg route to a random adjacent cell — the shake-loose hop for a
    // stalled actor.
    pub(crate) fn random_neighbor_route(&self, start: &Position, rng: &mut impl Rng) -> Option<PlannedRoute> {
        let node = self.node_for_position(start)?;
        let neighbors = self.neighbors(node);
        if neighbors.is_empty() {
            return None;
        }
        let target = neighbors[rng.random_range(0..neighbors.len())];
        Some(PlannedRoute {
            waypoints: VecDeque::from([self.node_center(target)]),
            destination_node: target,
        })
    }

    pub(crate) fn safe_cover_route(
        &self,
        start: &Position,
        threats: &[Position],
        is_stable_cover: impl FnMut(&Position) -> bool + Copy,
    ) -> Option<PlannedRoute> {
        let start_node = self.node_for_position(start)?;
        for exclusion_cells in [
            PRIMARY_THREAT_EXCLUSION_CELLS,
            FALLBACK_THREAT_EXCLUSION_CELLS,
            MINIMUM_THREAT_EXCLUSION_CELLS,
        ] {
            let exclusion_radius = exclusion_cells * self.cell_size();
            if let Some(route) = self.nearest_cover_route(start_node, threats, exclusion_radius, is_stable_cover) {
                return Some(route);
            }
        }
        self.safest_reachable_route(start_node, threats)
    }

    fn nearest_cover_route(
        &self,
        start: NavNode,
        threats: &[Position],
        exclusion_radius: f32,
        mut is_stable_cover: impl FnMut(&Position) -> bool,
    ) -> Option<PlannedRoute> {
        let exclusion_sq = exclusion_radius * exclusion_radius;
        self.route_to_any_with_limit(
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

    fn safest_reachable_route(&self, start: NavNode, threats: &[Position]) -> Option<PlannedRoute> {
        let minimum_exclusion = MINIMUM_THREAT_EXCLUSION_CELLS * self.cell_size();
        let minimum_exclusion_sq = minimum_exclusion * minimum_exclusion;
        let search = self.reachable(
            start,
            |node| {
                node == start
                    || threats
                        .iter()
                        .all(|threat| self.node_center(node).horizontal_distance_sq(threat) >= minimum_exclusion_sq)
            },
            COVER_SEARCH_MAX_STEPS,
        );
        let target = search
            .depths
            .iter()
            .filter(|(node, _)| self.is_cover_destination(**node))
            .max_by(|(a, a_depth), (b, b_depth)| {
                minimum_threat_distance_sq(self.node_center(**a), threats)
                    .total_cmp(&minimum_threat_distance_sq(self.node_center(**b), threats))
                    .then_with(|| b_depth.cmp(a_depth))
                    .then_with(|| a.cmp(b))
            })
            .map(|(node, _)| *node)?;
        if target == start {
            return None;
        }
        self.route_from_search(start, target, &search.came_from)
    }

    fn route_between_nodes(
        &self,
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
        self.route_to_any(start, |node| node == target, allowed)
    }

    fn route_to_any(
        &self,
        start: NavNode,
        is_target: impl FnMut(NavNode) -> bool,
        allowed: impl Fn(NavNode) -> bool,
    ) -> Option<PlannedRoute> {
        self.route_to_any_with_limit(start, is_target, allowed, usize::MAX)
    }

    fn route_to_any_with_limit(
        &self,
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
                return self.route_from_search(start, node, &came_from);
            }
            let depth = depths[&node];
            if depth >= max_depth {
                continue;
            }
            for &next in self.neighbors(node) {
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

    fn reachable(&self, start: NavNode, allowed: impl Fn(NavNode) -> bool, max_depth: usize) -> ReachableSearch {
        let mut queue = VecDeque::from([start]);
        let mut came_from = HashMap::from([(start, None)]);
        let mut depths = HashMap::from([(start, 0usize)]);
        while let Some(node) = queue.pop_front() {
            let depth = depths[&node];
            if depth >= max_depth {
                continue;
            }
            for &next in self.neighbors(node) {
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
        Some(PlannedRoute {
            waypoints: nodes.into_iter().map(|node| self.node_center(node)).collect(),
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
