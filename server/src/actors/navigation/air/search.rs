use super::constants::ESCAPE_SEARCH_WORK;
use bevy::prelude::{IVec3, Vec3};
use common::{
    config::CharacterPhysicsConfig,
    physics::CollisionWorld,
    protocol::{BarrierId, Position},
};
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, VecDeque},
};

#[derive(Clone, Copy)]
pub(super) struct Frontier {
    pub node: IVec3,
    pub cost: f32,
    pub priority: f32,
}
impl PartialEq for Frontier {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.node == other.node
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
            .priority
            .total_cmp(&self.priority)
            .then_with(|| self.node.to_array().cmp(&other.node.to_array()))
    }
}

pub(crate) enum SearchResult {
    Pending,
    Found(VecDeque<Position>),
    Unreachable,
}

pub(crate) struct AirSearch {
    start: Vec3,
    target: Vec3,
    spacing: f32,
    frontier: BinaryHeap<Frontier>,
    costs: HashMap<IVec3, f32>,
    parents: HashMap<IVec3, IVec3>,
    extent: f32,
    touched_boundary: bool,
    bounds: Option<(Vec3, Vec3)>,
    expanded: usize,
}

impl AirSearch {
    pub fn new(start: Position, target: Position, spacing: f32) -> Self {
        let start = Vec3::from(start);
        let target = Vec3::from(target);
        let mut search = Self {
            start,
            target,
            spacing,
            frontier: BinaryHeap::new(),
            costs: HashMap::new(),
            parents: HashMap::new(),
            extent: spacing * 4.0,
            touched_boundary: false,
            bounds: None,
            expanded: 0,
        };
        search.reset();
        search
    }

    pub fn destination(&self) -> Position {
        self.target.into()
    }

    fn reset(&mut self) {
        self.frontier.clear();
        self.costs.clear();
        self.parents.clear();
        self.touched_boundary = false;
        self.costs.insert(IVec3::ZERO, 0.0);
        self.frontier.push(Frontier {
            node: IVec3::ZERO,
            cost: 0.0,
            priority: self.start.distance(self.target),
        });
    }

    pub fn advance(
        &mut self,
        world: &CollisionWorld,
        physics: CharacterPhysicsConfig,
        open: &[BarrierId],
        budget: &mut usize,
        allowed: impl Fn(Vec3) -> bool,
    ) -> SearchResult {
        let target = self.target;
        self.advance_to_goal(world, physics, open, budget, allowed, |point| {
            point.distance_squared(target) < 0.0001
        })
    }

    pub fn advance_to_goal(
        &mut self,
        world: &CollisionWorld,
        physics: CharacterPhysicsConfig,
        open: &[BarrierId],
        budget: &mut usize,
        allowed: impl Fn(Vec3) -> bool,
        arrived: impl Fn(Vec3) -> bool,
    ) -> SearchResult {
        if world.character_overlaps_solid(&self.start.into(), physics, open) {
            return SearchResult::Unreachable;
        }
        let (world_min, world_max) = *self.bounds.get_or_insert_with(|| {
            world.flight_search_bounds(
                self.start,
                self.target,
                physics.movement_collider.height + self.spacing * 2.0,
            )
        });
        while *budget > 0 {
            let Some(current) = self.frontier.pop() else {
                if self.touched_boundary {
                    self.extent *= 2.0;
                    self.reset();
                    return SearchResult::Pending;
                }
                return SearchResult::Unreachable;
            };
            *budget -= 1;
            self.expanded += 1;
            if current.cost > self.costs[&current.node] {
                continue;
            }
            let point = self.point(current.node);
            let end = if arrived(point) && !world.character_overlaps_solid(&point.into(), physics, open) {
                Some(point)
            } else if arrived(self.target)
                && world.character_flight_path_clear(point.into(), self.target.into(), physics, open)
                && segment_allowed(point, self.target, self.spacing, &allowed)
            {
                Some(self.target)
            } else {
                None
            };
            if let Some(end) = end {
                if let Some(path) = self.route(current.node, end, world, physics, open) {
                    return SearchResult::Found(path);
                }
                self.reset();
                return SearchResult::Pending;
            }
            for offset in neighbors() {
                let next = current.node + offset;
                let point_next = self.point(next);
                if !point_next.cmpge(world_min).all() || !point_next.cmple(world_max).all() {
                    continue;
                }
                let min = self.start.min(self.target) - Vec3::splat(self.extent);
                let max = self.start.max(self.target) + Vec3::splat(self.extent);
                if !point_next.cmpge(min).all() || !point_next.cmple(max).all() {
                    self.touched_boundary = true;
                    continue;
                }
                if !segment_allowed(point, point_next, self.spacing * 0.5, &allowed) {
                    continue;
                }
                let cost = current.cost + offset.as_vec3().length() * self.spacing;
                if self.costs.get(&next).is_some_and(|previous| *previous <= cost) {
                    continue;
                }
                if !world.character_flight_path_clear(point.into(), point_next.into(), physics, open) {
                    continue;
                }
                self.costs.insert(next, cost);
                self.parents.insert(next, current.node);
                self.frontier.push(Frontier {
                    node: next,
                    cost,
                    priority: cost + point_next.distance(self.target),
                });
            }
        }
        SearchResult::Pending
    }

    pub fn advance_escape(
        &mut self,
        world: &CollisionWorld,
        physics: CharacterPhysicsConfig,
        open: &[BarrierId],
        budget: &mut usize,
        covered: impl Fn(Vec3) -> bool,
        safety: impl Fn(Vec3) -> f32,
    ) -> SearchResult {
        let start_safety = safety(self.start);
        let target = self.target;
        // Inaccessible cover must yield a reachable retreat before the actor spends its cooldown waiting.
        let allowance = (*budget).min(ESCAPE_SEARCH_WORK.saturating_sub(self.expanded));
        let mut work = allowance;
        let result = self.advance_to_goal(
            world,
            physics,
            open,
            &mut work,
            |point| safety(point) >= start_safety.min(physics.movement_collider.radius().powi(2) * 16.0) * 0.25,
            |point| covered(point) || (point.distance_squared(target) < 0.0001 && safety(point) > start_safety),
        );
        *budget -= allowance - work;
        match result {
            SearchResult::Found(_) => return result,
            SearchResult::Pending if self.expanded < ESCAPE_SEARCH_WORK => return result,
            _ => {}
        }
        let retreat = self
            .costs
            .keys()
            .copied()
            .filter(|node| *node != IVec3::ZERO)
            .map(|node| (node, safety(self.point(node))))
            .max_by(|(a, score_a), (b, score_b)| {
                score_a.total_cmp(score_b).then_with(|| a.to_array().cmp(&b.to_array()))
            });
        retreat
            .and_then(|(node, _)| self.route(node, self.point(node), world, physics, open))
            .map_or(SearchResult::Unreachable, SearchResult::Found)
    }

    fn route(
        &self,
        mut node: IVec3,
        end: Vec3,
        world: &CollisionWorld,
        physics: CharacterPhysicsConfig,
        open: &[BarrierId],
    ) -> Option<VecDeque<Position>> {
        let mut path = VecDeque::from([Position::from(end)]);
        if self.point(node).distance_squared(end) < 0.0001 && node != IVec3::ZERO {
            node = self.parents[&node];
        }
        while node != IVec3::ZERO {
            path.push_front(self.point(node).into());
            node = self.parents[&node];
        }
        let mut previous = self.start;
        path.iter()
            .all(|p| {
                let clear = world.character_flight_path_clear(previous.into(), *p, physics, open);
                previous = Vec3::from(*p);
                clear
            })
            .then_some(path)
    }

    fn point(&self, node: IVec3) -> Vec3 {
        self.start + node.as_vec3() * self.spacing
    }
}

pub(super) fn neighbors() -> impl Iterator<Item = IVec3> {
    (-1..=1).flat_map(|x| {
        (-1..=1).flat_map(move |y| {
            (-1..=1).filter_map(move |z| {
                let node = IVec3::new(x, y, z);
                (node != IVec3::ZERO).then_some(node)
            })
        })
    })
}

fn segment_allowed(from: Vec3, to: Vec3, spacing: f32, allowed: &impl Fn(Vec3) -> bool) -> bool {
    let steps = (from.distance(to) / spacing).ceil().max(1.0) as usize;
    (1..=steps).all(|i| allowed(from.lerp(to, i as f32 / steps as f32)))
}

#[cfg(test)]
#[path = "tests/search.rs"]
mod tests;
