use std::collections::VecDeque;

use bevy::prelude::*;
use common::protocol::sequence_is_newer;

use crate::constants::TICK_SYNC_WINDOW_TICKS;

// Corrects the client's `ServerTick` from the server's echoes of its own
// commits: an echo names the server tick that applied a `CMove`, the
// committed-position ring holds the tick the client simulated it at, and the
// difference is the clock error. The first echo seeds the clock outright.
// After that a shift needs a full window of consecutive echoes all reporting
// a nonzero error in the same direction, and applies the smallest of them: a
// commit whose delivery jitters is applied a tick late, and a clock that
// followed each echo would flap by a tick. Two kinds of echo are not
// measured: one repeating the last seq measured (the server waited for a
// lost commit, so its tick is late by the wait, not by the clock), and one
// echoing a commit made before the last shift (it still carries the error
// that shift removed).
#[derive(Resource, Default)]
pub struct TickSync {
    seeded: bool,
    roughly_seeded: bool,
    window: VecDeque<i32>,
    last_measured_seq: u32,
    ignore_through_seq: u32,
}

impl TickSync {
    // Whether the clock still needs its rough seed: until the first own echo
    // measures it, the first state message puts the clock near the server's
    // tick, so a joining client does not see the carriers at tick zero
    // for a round trip. True once, before any echo.
    pub fn takes_rough_seed(&mut self) -> bool {
        if self.seeded || self.roughly_seeded {
            return false;
        }
        self.roughly_seeded = true;
        true
    }

    // `error` is the echoed tick minus the recorded one for `echoed_seq`;
    // `committed_seq` is the newest commit made so far. Returns the shift to
    // apply to the clock.
    pub fn observe(&mut self, error: i32, echoed_seq: u32, committed_seq: u32) -> Option<i32> {
        if self.seeded
            && (!sequence_is_newer(echoed_seq, self.ignore_through_seq)
                || !sequence_is_newer(echoed_seq, self.last_measured_seq))
        {
            return None;
        }
        let required = if self.seeded { TICK_SYNC_WINDOW_TICKS } else { 1 };
        self.seeded = true;
        self.last_measured_seq = echoed_seq;
        if error == 0
            || self
                .window
                .front()
                .is_some_and(|first| first.signum() != error.signum())
        {
            self.window.clear();
        }
        if error == 0 {
            return None;
        }
        self.window.push_back(error);
        if self.window.len() < required {
            return None;
        }
        let shift = self.window.iter().copied().min_by_key(|error| error.abs())?;
        self.window.clear();
        self.ignore_through_seq = committed_seq;
        Some(shift)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> TickSync {
        let mut sync = TickSync::default();
        assert_eq!(sync.observe(0, 1, 1), None);
        sync
    }

    #[test]
    fn a_rough_seed_is_taken_once_before_the_first_echo() {
        let mut sync = TickSync::default();
        assert!(sync.takes_rough_seed());
        assert!(!sync.takes_rough_seed());
        assert_eq!(sync.observe(3, 1, 1), Some(3));
        let mut seeded = seeded();
        assert!(!seeded.takes_rough_seed());
    }

    #[test]
    fn first_echo_seeds_the_clock() {
        let mut sync = TickSync::default();
        assert_eq!(sync.observe(37, 1, 1), Some(37));
    }

    #[test]
    fn a_constant_error_shifts_by_it_after_the_window() {
        let mut sync = seeded();
        for seq in 2..TICK_SYNC_WINDOW_TICKS as u32 + 1 {
            assert_eq!(sync.observe(1, seq, seq), None);
        }
        let last = TICK_SYNC_WINDOW_TICKS as u32 + 1;
        assert_eq!(sync.observe(1, last, last), Some(1));
    }

    #[test]
    fn jitter_shifts_by_the_smallest_error() {
        let mut sync = seeded();
        let mut shift = None;
        for seq in 2..2 + TICK_SYNC_WINDOW_TICKS as u32 {
            let error = if seq % 2 == 0 { 2 } else { 1 };
            shift = shift.or(sync.observe(error, seq, seq));
        }
        assert_eq!(shift, Some(1));
    }

    #[test]
    fn mixed_signs_never_shift() {
        let mut sync = seeded();
        for seq in 2..2 + 4 * TICK_SYNC_WINDOW_TICKS as u32 {
            let error = if seq % 2 == 0 { 1 } else { -1 };
            assert_eq!(sync.observe(error, seq, seq), None);
        }
    }

    #[test]
    fn echoes_committed_before_a_shift_are_ignored() {
        let mut sync = TickSync::default();
        assert_eq!(sync.observe(5, 1, 10), Some(5));
        for seq in 2..=10 {
            assert_eq!(sync.observe(5, seq, 10), None);
        }
        assert_eq!(sync.observe(5, 11, 11), None);
        assert!(sync.window.len() == 1);
    }

    #[test]
    fn a_repeated_seq_is_not_measured() {
        let mut sync = seeded();
        for _ in 0..2 * TICK_SYNC_WINDOW_TICKS {
            assert_eq!(sync.observe(1, 2, 2), None);
        }
        assert!(sync.window.len() <= 1);
    }
}
