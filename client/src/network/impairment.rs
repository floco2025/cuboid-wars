use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use rand::{RngExt, rng};

#[derive(Debug, Clone, Copy, Default)]
pub struct Impairment {
    pub lag: Duration,
    pub jitter: f32,
    pub drop_probability: f32,
}

impl Impairment {
    pub(super) fn delays(&self) -> bool {
        !self.lag.is_zero()
    }

    pub(super) fn drops(&self) -> bool {
        self.drop_probability > 0.0 && rng().random_bool(f64::from(self.drop_probability))
    }

    pub(super) fn delay(&self, unreliable: bool) -> Duration {
        if !unreliable || self.jitter == 0.0 || self.lag.is_zero() {
            return self.lag;
        }
        let jitter = f64::from(self.jitter);
        self.lag.mul_f64(rng().random_range((1.0 - jitter)..=(1.0 + jitter)))
    }
}

// Messages waiting out a simulated delay, released in deadline order so a
// shorter delay overtakes a longer one queued before it.
pub struct DelayQueue<T> {
    queue: VecDeque<(Instant, T)>,
}

impl<T> Default for DelayQueue<T> {
    fn default() -> Self {
        Self { queue: VecDeque::new() }
    }
}

impl<T> DelayQueue<T> {
    pub fn push(&mut self, now: Instant, delay: Duration, item: T) {
        let due = now + delay;
        let index = self.queue.partition_point(|(queued, _)| *queued <= due);
        self.queue.insert(index, (due, item));
    }

    pub fn pop_due(&mut self, now: Instant) -> Option<T> {
        let (due, _) = self.queue.front()?;
        if *due > now {
            return None;
        }
        self.queue.pop_front().map(|(_, item)| item)
    }
}

#[cfg(test)]
#[path = "tests/impairment.rs"]
mod tests;
