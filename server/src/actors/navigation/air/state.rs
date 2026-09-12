use super::AirSearch;
use common::protocol::{PlayerId, Position};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlightTask {
    Roam,
    Return,
    Pursue(PlayerId),
    Evade,
}

#[derive(Default)]
pub(crate) struct FlightState {
    pub route: VecDeque<Position>,
    pub search: Option<AirSearch>,
    pub task: Option<FlightTask>,
    pub retry_secs: f32,
    pub unreachable: HashMap<PlayerId, Position>,
    pub unreachable_secs: f32,
    pub recovery_secs: f32,
}

impl FlightState {
    pub fn clear(&mut self) {
        self.route.clear();
        self.search = None;
        self.task = None;
    }
}
