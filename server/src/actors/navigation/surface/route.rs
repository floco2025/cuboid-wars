use std::{
    cmp::Ordering,
    collections::{BinaryHeap, VecDeque},
};

use common::protocol::Position;

use super::{SurfaceLocation, SurfaceMesh, TraversalAction};

// The most search, visibility, and funnel visits one route may spend.
pub(crate) const ROUTE_SEARCH_VISITS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteFailure {
    StartOutsideMesh,
    GoalOutsideMesh,
    Disconnected,
    DifferentCarrier,
    OutsideTerritory,
    BodyBlocked,
    NavigationUnavailable,
    SearchLimit,
}

#[derive(Debug, Clone)]
pub struct SurfaceRoute {
    pub actions: VecDeque<TraversalAction>,
    pub expanded: usize,
}

#[derive(Clone, Copy)]
enum Transition {
    Walk(usize),
    Ladder(usize),
}

#[derive(Clone, Copy)]
struct Frontier {
    polygon: usize,
    cost: f32,
    estimate: f32,
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
            .then_with(|| other.polygon.cmp(&self.polygon))
    }
}

impl SurfaceMesh {
    #[cfg(test)]
    pub fn route(
        &self,
        start: Position,
        goal: Position,
        projection_distance: f32,
    ) -> Result<SurfaceRoute, RouteFailure> {
        self.route_for(start, goal, projection_distance, ROUTE_SEARCH_VISITS, false)
    }

    #[cfg(test)]
    pub fn route_with_limit(
        &self,
        start: Position,
        goal: Position,
        projection_distance: f32,
        limit: usize,
    ) -> Result<SurfaceRoute, RouteFailure> {
        self.route_for(start, goal, projection_distance, limit, false)
    }

    pub(crate) fn route_for(
        &self,
        start: Position,
        goal: Position,
        projection_distance: f32,
        limit: usize,
        ladders: bool,
    ) -> Result<SurfaceRoute, RouteFailure> {
        let start = self
            .locate(start, projection_distance)
            .ok_or(RouteFailure::StartOutsideMesh)?;
        let end = self
            .locate(goal, projection_distance)
            .ok_or(RouteFailure::GoalOutsideMesh)?;
        if !self.connected(start, end, ladders) {
            return Err(RouteFailure::Disconnected);
        }
        let mut remaining = limit;
        let mut actions = VecDeque::from([TraversalAction::Walk {
            carrier: self.carrier,
            target: start.position,
        }]);
        if self.direct_walk(start, end, &mut remaining)? {
            actions.push_back(TraversalAction::Walk {
                carrier: self.carrier,
                target: end.position,
            });
            return Ok(SurfaceRoute {
                actions,
                expanded: limit - remaining,
            });
        }
        let mut costs = vec![f32::INFINITY; self.polygon_count()];
        let mut parents = vec![None; self.polygon_count()];
        costs[start.polygon] = 0.0;
        let end_center = self.center(end.polygon);
        let mut queue = BinaryHeap::from([Frontier {
            polygon: start.polygon,
            cost: 0.0,
            estimate: 0.0,
        }]);
        while let Some(Frontier { polygon, cost, .. }) = queue.pop() {
            if cost > costs[polygon] {
                continue;
            }
            remaining = remaining.checked_sub(1).ok_or(RouteFailure::SearchLimit)?;
            if polygon == end.polygon {
                break;
            }
            let center = self.center(polygon);
            let mut visit = |next: usize, transition: Transition, distance: f32| {
                let next_cost = cost + distance;
                if next_cost >= costs[next] {
                    return;
                }
                costs[next] = next_cost;
                parents[next] = Some((polygon, transition));
                let estimate = next_cost + self.center(next).distance_sq(&end_center).sqrt();
                queue.push(Frontier {
                    polygon: next,
                    cost: next_cost,
                    estimate,
                });
            };
            for (edge, next) in self.neighbors[polygon].iter().enumerate() {
                if let Some(next) = *next {
                    visit(
                        next,
                        Transition::Walk(edge),
                        center.distance_sq(&self.center(next)).sqrt(),
                    );
                }
            }
            if ladders {
                for (index, link) in self.links[polygon].iter().enumerate() {
                    let distance = center.distance_sq(&link.from.position).sqrt()
                        + link.from.position.distance_sq(&link.to.position).sqrt()
                        + link.to.position.distance_sq(&self.center(link.to.polygon)).sqrt();
                    visit(link.to.polygon, Transition::Ladder(index), distance);
                }
            }
        }
        if start.polygon != end.polygon && parents[end.polygon].is_none() {
            return Err(RouteFailure::Disconnected);
        }
        let mut segments = Vec::new();
        let mut cursor = end.polygon;
        while cursor != start.polygon {
            let (previous, transition) = parents[cursor].expect("parent missing from a reached route polygon");
            segments.push((previous, transition));
            cursor = previous;
        }
        let mut walk_start = start;
        let mut portals = Vec::new();
        for (previous, transition) in segments.into_iter().rev() {
            match transition {
                Transition::Walk(edge) => portals.push(self.portal(previous, edge)),
                Transition::Ladder(index) => {
                    let link = &self.links[previous][index];
                    actions.extend(self.corridor_actions(walk_start, link.from, &portals, &mut remaining));
                    actions.extend(link.actions.iter().copied());
                    portals.clear();
                    walk_start = link.to;
                }
            }
        }
        actions.extend(self.corridor_actions(walk_start, end, &portals, &mut remaining));
        Ok(SurfaceRoute {
            actions,
            expanded: limit - remaining,
        })
    }

    pub(crate) fn connected(&self, a: SurfaceLocation, b: SurfaceLocation, ladders: bool) -> bool {
        let components = &self.components[usize::from(ladders)];
        components
            .get(a.polygon)
            .zip(components.get(b.polygon))
            .is_some_and(|(a, b)| a == b)
    }

    pub(super) fn update_components(&mut self) {
        for ladders in [false, true] {
            let mut components = vec![usize::MAX; self.polygon_count()];
            for seed in 0..self.polygon_count() {
                if components[seed] != usize::MAX {
                    continue;
                }
                components[seed] = seed;
                let mut queue = VecDeque::from([seed]);
                while let Some(polygon) = queue.pop_front() {
                    let next = self.neighbors[polygon].iter().flatten().copied().chain(
                        self.links[polygon]
                            .iter()
                            .filter(|_| ladders)
                            .map(|link| link.to.polygon),
                    );
                    for next in next {
                        if components[next] == usize::MAX {
                            components[next] = seed;
                            queue.push_back(next);
                        }
                    }
                }
            }
            self.components[usize::from(ladders)] = components;
        }
    }
}

#[cfg(test)]
#[path = "tests/route.rs"]
mod tests;
