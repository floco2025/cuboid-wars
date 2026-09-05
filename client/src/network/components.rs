use bevy::prelude::*;
use common::protocol::Position;

use crate::network::resources::RoundTripTime;

// ============================================================================
// Components
// ============================================================================

// The server's authoritative position for this entity and the gap to close
// toward it: `recorded_correction` for the local player,
// `extrapolated_correction` for everything else.
#[derive(Component)]
pub struct ServerReconciliation {
    pub correction_delta: Vec3,
    pub server_pos: Position,
    pub server_velocity: Vec3,
    pub correction_progress: f32,
    pub rtt: f32,
}

impl ServerReconciliation {
    // RTT captured as seconds (centralizing the `Duration → f32` conversion).
    #[must_use]
    pub fn new(correction_delta: Vec3, server_pos: Position, server_velocity: Vec3, rtt: &RoundTripTime) -> Self {
        Self {
            correction_delta,
            server_pos,
            server_velocity,
            correction_progress: 0.0,
            rtt: rtt.rtt.as_secs_f32(),
        }
    }
}

// Gap to close for the local player: the server position after a `CMove`
// minus where our own simulation stood after that same `CMove`, so the
// difference is the prediction error alone.
#[must_use]
pub fn recorded_correction(recorded_pos: Position, server_pos: Position) -> Vec3 {
    Vec3::from(server_pos) - Vec3::from(recorded_pos)
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

// Pick the axis with the largest |value| from a 3-component delta. Used
// for the per-axis snap decision and its warning log, so the reader sees
// which axis tripped the threshold.
pub fn worst_axis_divergence(delta: Vec3) -> (&'static str, f32) {
    let xa = delta.x.abs();
    let ya = delta.y.abs();
    let za = delta.z.abs();
    if xa >= ya && xa >= za {
        ("x", xa)
    } else if ya >= za {
        ("y", ya)
    } else {
        ("z", za)
    }
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
    fn recorded_correction_is_the_offset_between_the_two_positions() {
        assert_eq!(
            recorded_correction(pos(1.0, 0.0, 0.0), pos(3.0, 0.0, 0.0)),
            Vec3::new(2.0, 0.0, 0.0)
        );
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

    #[test]
    fn divergence_picks_the_dominant_axis() {
        assert_eq!(worst_axis_divergence(Vec3::new(-3.0, 1.0, 2.0)), ("x", 3.0));
        assert_eq!(worst_axis_divergence(Vec3::new(1.0, -3.0, 2.0)), ("y", 3.0));
        assert_eq!(worst_axis_divergence(Vec3::new(1.0, 2.0, -3.0)), ("z", 3.0));
    }

    #[test]
    fn divergence_ties_prefer_x_then_y() {
        assert_eq!(worst_axis_divergence(Vec3::splat(2.0)), ("x", 2.0));
        assert_eq!(worst_axis_divergence(Vec3::new(0.0, 2.0, 2.0)), ("y", 2.0));
    }
}
