use bevy::prelude::*;
use common::protocol::Position;

// Snapshot of `Position` at the start of the most recent fixed-physics
// tick. Physics runs at 30 Hz, rendering at the display rate; transform-
// sync systems lerp between this and the current `Position` using
// `Time<Fixed>::overstep_fraction()` so motion stays smooth above 30 Hz.
#[derive(Component, Default, Clone, Copy)]
pub struct PreviousTickPosition(pub Position);

impl PreviousTickPosition {
    // Interpolate from this previous-tick position to `curr` using the
    // fixed-step overstep fraction.
    #[must_use]
    pub fn lerp_to(&self, curr: Position, alpha: f32) -> Vec3 {
        Vec3::from(self.0).lerp(curr.into(), alpha)
    }
}
