use bevy::prelude::*;
use common::protocol::Position;

use crate::{
    constants::{RECON_CORRECTION_MIN_SECS, RECON_CORRECTION_TIME_RTT_MULTIPLIER},
    network::resources::RoundTripTime,
};

// ============================================================================
// Components
// ============================================================================

// The gap a remote character or missile still has to close toward its last
// server sample, bled off over a correction window. Each consumer keeps its
// own snap threshold; a gap past it is taken as a cut to `server_pos`, since
// feeding it back through delayed updates would never converge.
#[derive(Component)]
pub struct ServerReconciliation {
    pub correction_delta: Vec3,
    pub server_pos: Position,
    pub server_velocity: Vec3,
    pub applied_fraction: f32,
    rtt: f32,
}

impl ServerReconciliation {
    // Updates replace this component, so in steady state this is an
    // exponential pull toward a moving target; when the stream pauses the
    // last correction finishes linearly, capped so it cannot overshoot.
    pub fn correction_fraction(&mut self, delta: f32) -> f32 {
        let window = (self.rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER).max(RECON_CORRECTION_MIN_SECS);
        let fraction = (delta / window).min(1.0 - self.applied_fraction);
        self.applied_fraction += fraction;
        fraction
    }

    // RTT captured as seconds (centralizing the `Duration → f32` conversion).
    #[must_use]
    pub fn new(correction_delta: Vec3, server_pos: Position, server_velocity: Vec3, rtt: &RoundTripTime) -> Self {
        Self {
            correction_delta,
            server_pos,
            server_velocity,
            applied_fraction: 0.0,
            rtt: rtt.rtt.as_secs_f32(),
        }
    }
}

// Gap to close for a sample with no `CMove` of ours behind it (remote
// players, actors, missiles): the server position is ~half an RTT old, so it
// is projected forward by that much before the current client position is
// subtracted.
#[must_use]
pub fn extrapolated_correction(
    client_pos: Position,
    server_pos: Position,
    server_velocity: Vec3,
    rtt: &RoundTripTime,
) -> Vec3 {
    Vec3::from(server_pos) + server_velocity * rtt.rtt.as_secs_f32() / 2.0 - Vec3::from(client_pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn pos(x: f32, y: f32, z: f32) -> Position {
        Position { x, y, z }
    }

    fn rtt_ms(millis: u64) -> RoundTripTime {
        RoundTripTime {
            rtt: Duration::from_millis(millis),
            ..Default::default()
        }
    }

    #[test]
    fn smoothing_finishes_without_overshoot_when_updates_pause() {
        for millis in [0, 200, 1000] {
            let mut recon = ServerReconciliation::new(Vec3::X, pos(1.0, 0.0, 0.0), Vec3::ZERO, &rtt_ms(millis));
            let mut applied = 0.0;
            for _ in 0..300 {
                applied += recon.correction_fraction(1.0 / 30.0);
            }
            assert!((applied - 1.0).abs() < 1e-6);
            assert_eq!(recon.applied_fraction, 1.0);
            assert_eq!(recon.correction_fraction(1.0 / 30.0), 0.0);
        }
    }

    #[test]
    fn smoothing_window_scales_with_rtt_and_keeps_a_minimum() {
        for (millis, window) in [(0, 0.25), (200, 0.8)] {
            let mut recon = ServerReconciliation::new(Vec3::X, pos(1.0, 0.0, 0.0), Vec3::ZERO, &rtt_ms(millis));
            assert!((recon.correction_fraction(1.0 / 30.0) - (1.0 / 30.0) / window).abs() < 1e-6);
        }
    }

    #[test]
    fn extrapolated_correction_without_velocity_is_the_plain_offset() {
        let delta = extrapolated_correction(pos(1.0, 0.0, 0.0), pos(3.0, 0.0, 0.0), Vec3::ZERO, &rtt_ms(200));
        assert_eq!(delta, Vec3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn extrapolated_correction_projects_server_forward_by_half_rtt() {
        let delta = extrapolated_correction(
            pos(0.0, 0.0, 0.0),
            pos(0.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
            &rtt_ms(200),
        );
        assert_eq!(delta, Vec3::new(1.0, 0.0, 0.0));
    }
}
