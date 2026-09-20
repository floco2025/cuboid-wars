use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, VecDeque},
};

use bevy::math::{IVec3, Vec3};
use common::{map::Carriers, physics::CollisionWorld, protocol::FieldId};

use super::{
    air_graph::{AirGraph, AirNode},
    steering::{sweep_clear, terminal_approach},
};
use crate::constants::{MISSILE_SEARCH_MISSILE_QUERIES, MISSILE_SEARCH_NODE_LIMIT, MISSILE_SEARCH_WINDOW_REACH_CELLS};

pub(crate) struct SearchBudget {
    pub remaining: usize,
    pub used: usize,
}
impl SearchBudget {
    pub fn new(queries: usize) -> Self {
        Self {
            remaining: queries,
            used: 0,
        }
    }
    fn spend(&mut self, amount: usize, slice: &mut usize) -> bool {
        if amount > self.remaining.min(*slice) {
            return false;
        }
        self.remaining -= amount;
        self.used += amount;
        *slice -= amount;
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteStatus {
    Idle,
    Pending,
    Found,
    Unreachable,
    Limited,
}

pub(crate) enum SearchProgress {
    Pending,
    Found(VecDeque<Vec3>),
    Unreachable,
    // The window's edge stopped the search: a wider window may still find a route.
    WindowLimited,
    NodeLimited,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Node {
    Origin,
    Air(AirNode),
}

struct Frontier {
    node: Node,
    cost: f32,
    estimate: f32,
    order: usize,
}
impl PartialEq for Frontier {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Frontier {}
impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then_with(|| other.order.cmp(&self.order))
    }
}

struct Expansion {
    node: Node,
    neighbors: Option<VecDeque<AirNode>>,
}

pub(crate) struct AirSearch {
    pub target: Vec3,
    pub open: Vec<FieldId>,
    radius: f32,
    fuse_distance: f32,
    min: IVec3,
    max: IVec3,
    queue: BinaryHeap<Frontier>,
    // Carrier nodes stay local while a search waits. Guidance validates the
    // resulting world-space route against the current collision world.
    reached: HashMap<Node, (Option<Node>, Vec3, f32)>,
    active: Option<Expansion>,
    touched_boundary: bool,
}

impl AirSearch {
    pub fn new(
        graph: &AirGraph,
        carriers: &Carriers,
        open: &[FieldId],
        from: Vec3,
        to: Vec3,
        radius: f32,
        fuse_distance: f32,
        margin_cells: i32,
    ) -> Self {
        let a = graph.node_at(carriers, 0, from);
        let b = graph.node_at(carriers, 0, to);
        let a = IVec3::new(a.col, a.layer, a.row);
        let b = IVec3::new(b.col, b.layer, b.row).clamp(
            a - IVec3::splat(MISSILE_SEARCH_WINDOW_REACH_CELLS),
            a + IVec3::splat(MISSILE_SEARCH_WINDOW_REACH_CELLS),
        );
        Self {
            target: to,
            open: open.to_vec(),
            radius,
            fuse_distance,
            min: a.min(b) - IVec3::splat(margin_cells),
            max: a.max(b) + IVec3::splat(margin_cells),
            queue: BinaryHeap::from([Frontier {
                node: Node::Origin,
                cost: 0.0,
                estimate: from.distance(to),
                order: 0,
            }]),
            reached: HashMap::from([(Node::Origin, (None, from, 0.0))]),
            active: None,
            touched_boundary: false,
        }
    }

    pub fn advance(
        &mut self,
        graph: &AirGraph,
        carriers: &Carriers,
        world: &CollisionWorld,
        budget: &mut SearchBudget,
    ) -> SearchProgress {
        let mut slice = MISSILE_SEARCH_MISSILE_QUERIES;
        loop {
            if self.active.is_none() {
                let Some(frontier) = self.queue.pop() else {
                    return if self.touched_boundary {
                        SearchProgress::WindowLimited
                    } else {
                        SearchProgress::Unreachable
                    };
                };
                if frontier.cost > self.reached[&frontier.node].2 {
                    continue;
                }
                self.active = Some(Expansion {
                    node: frontier.node,
                    neighbors: None,
                });
            }
            let active = self
                .active
                .as_mut()
                .expect("expansion missing from an advancing search");
            let origin = match active.node {
                Node::Origin => self.reached[&Node::Origin].1,
                Node::Air(node) => graph.node_center(carriers, node),
            };
            if active.neighbors.is_none() {
                // Terminal approach performs at most three casts/rays; testing
                // an occupied origin adds one. Charge the conservative maximum.
                if !budget.spend(4, &mut slice) {
                    return SearchProgress::Pending;
                }
                if let Some(end) =
                    terminal_approach(world, &self.open, origin, self.target, self.radius, self.fuse_distance)
                {
                    let mut path = VecDeque::from([end]);
                    let mut node = active.node;
                    while let Some((Some(parent), point, _)) = self.reached.get(&node) {
                        let point = match node {
                            Node::Origin => *point,
                            Node::Air(node) => graph.node_center(carriers, node),
                        };
                        if path.front() != Some(&point) {
                            path.push_front(point);
                        }
                        node = *parent;
                    }
                    return SearchProgress::Found(path);
                }
                if active.node == Node::Origin && world.projectile_start_blocked(origin, self.radius, &self.open) {
                    return SearchProgress::Unreachable;
                }
                active.neighbors = Some(
                    match active.node {
                        Node::Origin => graph.endpoint_candidates(carriers, origin, self.min, self.max),
                        Node::Air(node) => graph.neighbors(carriers, node, self.min, self.max),
                    }
                    .into(),
                );
            }
            let neighbors = active
                .neighbors
                .as_mut()
                .expect("neighbors missing from an expanded node");
            while let Some(&next) = neighbors.front() {
                let key = Node::Air(next);
                let point = graph.node_center(carriers, next);
                let cost = self.reached[&active.node].2 + origin.distance(point);
                if self.reached.get(&key).is_some_and(|record| record.2 <= cost) {
                    neighbors.pop_front();
                    continue;
                }
                if !budget.spend(2, &mut slice) {
                    return SearchProgress::Pending;
                }
                neighbors.pop_front();
                if sweep_clear(world, &self.open, origin, point - origin, self.radius) {
                    if !self.reached.contains_key(&key) && self.reached.len() >= MISSILE_SEARCH_NODE_LIMIT {
                        return SearchProgress::NodeLimited;
                    }
                    let cell = IVec3::new(next.col, next.layer, next.row);
                    self.touched_boundary |=
                        next.grid == 0 && (cell.cmpeq(self.min).any() || cell.cmpeq(self.max).any());
                    self.reached.insert(key, (Some(active.node), point, cost));
                    self.queue.push(Frontier {
                        node: key,
                        cost,
                        estimate: cost + point.distance(self.target),
                        order: self.reached.len(),
                    });
                }
            }
            self.active = None;
        }
    }
}

#[cfg(test)]
#[path = "tests/search.rs"]
mod tests;
