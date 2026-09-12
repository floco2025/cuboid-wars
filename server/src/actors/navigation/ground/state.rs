use common::protocol::{BarrierId, CarrierId, PlayerId, Position};

use super::{GroundNavigation, GroundSearch, GroundSearchResult};

const FAILED_ROUTE_RETRY_SECS: f32 = 1.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroundTask {
    Roam,
    Return,
    Pursue(PlayerId),
    Evade(u8),
}

struct Query {
    task: GroundTask,
    target: Position,
    carrier: CarrierId,
    search: GroundSearch,
    revision: u64,
    open: Vec<BarrierId>,
}

struct Failure {
    task: GroundTask,
    target: Position,
    start: Position,
    revision: u64,
    open: Vec<BarrierId>,
    age: f32,
}

#[derive(Default)]
pub(crate) struct GroundState {
    query: Option<Query>,
    failures: Vec<Failure>,
    pub work: usize,
    pub seed: u32,
    pub evade_tier: u8,
}

impl GroundState {
    pub fn tick(&mut self, delta: f32, work: usize) {
        self.work = work;
        for failure in &mut self.failures {
            failure.age += delta;
        }
        self.failures.retain(|failure| {
            matches!(failure.task, GroundTask::Pursue(_) | GroundTask::Return) || failure.age < FAILED_ROUTE_RETRY_SECS
        });
    }

    pub fn clear(&mut self) {
        self.query = None;
        self.failures.clear();
    }

    pub fn retain_targets(&mut self, targets: impl Iterator<Item = PlayerId>) {
        let targets: Vec<_> = targets.collect();
        if self
            .query
            .as_ref()
            .is_some_and(|query| matches!(query.task, GroundTask::Pursue(id) if !targets.contains(&id)))
        {
            self.query = None;
        }
        self.failures
            .retain(|failure| !matches!(failure.task, GroundTask::Pursue(id) if !targets.contains(&id)));
    }

    pub fn pending(&self, task: GroundTask) -> bool {
        self.query.as_ref().is_some_and(|query| query.task == task)
    }

    pub fn route(
        &mut self,
        nav: &GroundNavigation<'_>,
        task: GroundTask,
        start: Position,
        target: Position,
        goal: impl Fn(Position, f32) -> Option<Position>,
        allowed: impl Fn(Position, Position) -> bool,
        fallback: Option<&dyn Fn(Position) -> f32>,
        limit: Option<usize>,
    ) -> GroundSearchResult {
        let revision = nav.world.geometry_revision();
        if self.failures.iter().any(|failure| {
            failure.task == task
                && failure.target.distance_sq(&target) < 0.04
                && failure.start.distance_sq(&start) < 0.04
                && failure.revision == revision
                && failure.open == nav.open
        }) {
            return GroundSearchResult::Unreachable;
        }
        let replace = self.query.as_ref().is_none_or(|query| {
            query.task != task
                || query.carrier != nav.carrier
                || query.target.distance_sq(&target) > nav.graphs.get(nav.carrier).cell_size().powi(2) * 4.0
        });
        if replace {
            let Some(search) = nav.search(start) else {
                return GroundSearchResult::Unreachable;
            };
            self.query = Some(Query {
                task,
                target,
                carrier: nav.carrier,
                search,
                revision,
                open: nav.open.to_vec(),
            });
        }
        let query = self.query.as_mut().expect("ground search missing from active query");
        let result = nav.advance(&mut query.search, goal, &allowed, &mut self.work, fallback, limit);
        match result {
            GroundSearchResult::Pending => GroundSearchResult::Pending,
            GroundSearchResult::Found(mut route) => {
                self.query = None;
                if nav.join_route(start, &mut route, &allowed) {
                    GroundSearchResult::Found(route)
                } else {
                    GroundSearchResult::Pending
                }
            }
            GroundSearchResult::Unreachable => {
                let revision = query.revision;
                let open = query.open.clone();
                self.query = None;
                self.failures.retain(|failure| failure.task != task);
                self.failures.push(Failure {
                    task,
                    target,
                    start,
                    revision,
                    open,
                    age: 0.0,
                });
                GroundSearchResult::Unreachable
            }
        }
    }
}
