use std::time::Duration;

use bevy::prelude::*;
use common::protocol::ServerTick;

#[derive(Resource, Default)]
pub struct TickSync {
    seeded: bool,
    roughly_seeded: bool,
}

impl TickSync {
    // A state update places carriers near server time before the first round trip completes.
    pub fn takes_rough_seed(&mut self) -> bool {
        if self.seeded || self.roughly_seeded {
            return false;
        }
        self.roughly_seeded = true;
        true
    }

    pub fn observe(&mut self, tick: &mut ServerTick, pong_tick: u32, rtt: Duration, server_hz: u32) {
        let one_way_ticks = (rtt.as_secs_f64() * f64::from(server_hz) / 2.0).round() as u32;
        let target = pong_tick.wrapping_add(one_way_ticks);
        let error = target.wrapping_sub(tick.0) as i32;
        // One tick of quantization is normal between independent fixed-step loops.
        if !self.seeded || error.unsigned_abs() > 1 {
            tick.0 = target;
        }
        self.seeded = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pong_seeds_after_a_rough_estimate_and_includes_return_latency() {
        let mut sync = TickSync::default();
        assert!(sync.takes_rough_seed());
        assert!(!sync.takes_rough_seed());
        let mut tick = ServerTick(1);
        sync.observe(&mut tick, 100, Duration::from_millis(200), 30);
        assert_eq!(tick.0, 103);
        assert!(!sync.takes_rough_seed());
    }

    #[test]
    fn tick_quantization_is_ignored_but_clock_drift_is_corrected_across_wrap() {
        let mut sync = TickSync::default();
        let mut tick = ServerTick(u32::MAX);
        sync.observe(&mut tick, u32::MAX, Duration::ZERO, 30);
        sync.observe(&mut tick, 0, Duration::ZERO, 30);
        assert_eq!(tick.0, u32::MAX);
        sync.observe(&mut tick, 1, Duration::ZERO, 30);
        assert_eq!(tick.0, 1);
        sync.observe(&mut tick, 0, Duration::ZERO, 30);
        assert_eq!(tick.0, 1);
        sync.observe(&mut tick, u32::MAX, Duration::ZERO, 30);
        assert_eq!(tick.0, u32::MAX);
    }
}
