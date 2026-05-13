use bevy::prelude::*;
use common::protocol::Position;

#[derive(Component)]
pub struct CharacterVisualTurnState {
    pub start_yaw: f32,
    pub target_yaw: f32,
    pub elapsed: f32,
    pub duration: f32,
}

impl CharacterVisualTurnState {
    pub const fn settled(yaw: f32) -> Self {
        Self {
            start_yaw: yaw,
            target_yaw: yaw,
            elapsed: 0.0,
            duration: 0.0,
        }
    }
}

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
