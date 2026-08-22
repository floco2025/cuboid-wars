use bevy::prelude::*;
use common::{
    constants::SNAPSHOT_SECS,
    physics::CharacterVerticalVelocity,
    protocol::{PlayerId, Position},
};

use crate::{
    characters::PreviousTickPosition,
    constants::{
        RECON_CORRECTION_TIME_RTT_MULTIPLIER, RECON_PLAYER_IDLE_CORRECTION_SECS, RECON_PLAYER_SNAP_DECAY_SECS,
        RECON_PLAYER_SNAP_DISTANCE_IDLE, RECON_PLAYER_SNAP_DISTANCE_RUNNING,
    },
    network::{ServerReconciliation, worst_axis_divergence},
};

pub(super) enum PlayerReconciliationOutcome {
    Displacement(Vec3),
    Snapped,
}

pub(super) fn reconcile_player(
    commands: &mut Commands,
    entity: Entity,
    player_id: &PlayerId,
    player_name: Option<&str>,
    client_pos: &mut Position,
    vertical_velocity: &mut CharacterVerticalVelocity,
    recon: &mut ServerReconciliation,
    control_velocity: Vec3,
    delta: f32,
    run_speed: f32,
    snap_speed: f32,
) -> PlayerReconciliationOutcome {
    // Vertical velocity counts toward motion — a jumping or falling player
    // with no horizontal input is still in motion.
    let motion_speed = control_velocity.x.hypot(control_velocity.z).hypot(vertical_velocity.0);
    let correction_factor = player_correction_factor(recon.rtt, motion_speed, run_speed);

    // Each tick applies `delta / correction window` of the fixed delta, so
    // the accumulator reaching `SNAPSHOT_SECS` coincides with exactly 100%
    // of the correction applied — removing the component here is what stops
    // over-correction, doubling as the dropped-snapshot fallback (normally
    // the next snapshot replaces this component first).
    recon.correction_progress += delta * correction_factor;
    if recon.correction_progress >= SNAPSHOT_SECS {
        commands.entity(entity).remove::<ServerReconciliation>();
    }

    let correction_delta = recon.extrapolated_delta();

    // Y is purely predicted: client owns jump intent and gravity, so vy is
    // locally authoritative. The snap branch still catches big disagreements
    // (e.g. landed-on-different-floor). Sub-threshold Y divergence persists
    // while moving (the threshold lerps up to the running distance) but
    // heals on stop: `snap_speed` decays back within
    // ~`RECON_PLAYER_SNAP_DECAY_SECS`, tightening the threshold to the idle
    // distance, so a floor-level disagreement snaps once the player
    // settles. Smoothing Y instead is not an option — the physics step only
    // takes the target's X/Z, and a nudged Y would be pushed back onto the
    // client-side floor by the support probe.
    let snap_threshold = player_snap_threshold(snap_speed, run_speed);
    let (worst_axis, worst_magnitude) = worst_axis_divergence(correction_delta);
    if worst_magnitude >= snap_threshold {
        let label = player_name.map_or_else(|| format!("{player_id:?}"), str::to_owned);
        warn!(
            "{label} out of sync: |{worst_axis}|={worst_magnitude:.2} >= {snap_threshold:.2} (Δ x={:.2}, y={:.2}, z={:.2}); snapping to server position",
            correction_delta.x, correction_delta.y, correction_delta.z
        );
        *client_pos = recon.server_pos;
        vertical_velocity.0 = recon.server_velocity.y;
        commands.entity(entity).remove::<ServerReconciliation>();
        // Keep render interpolation from smearing the snap across one frame.
        commands.entity(entity).insert(PreviousTickPosition(*client_pos));
        return PlayerReconciliationOutcome::Snapped;
    }

    PlayerReconciliationOutcome::Displacement(Vec3::new(
        correction_delta.x * delta * correction_factor / SNAPSHOT_SECS,
        0.0,
        correction_delta.z * delta * correction_factor / SNAPSHOT_SECS,
    ))
}

// Fraction of the correction delta applied per `SNAPSHOT_SECS` of real time
// — `SNAPSHOT_SECS / correction window`. The window lerps from the idle
// constant (long and gentle: stationary players see corrections clearly) to
// the RTT-scaled running window (short: motion hides the drift) by how fast
// the player is moving right now. A near-zero RTT saturates to 1.0 via the
// clamp.
fn player_correction_factor(rtt: f32, motion_speed: f32, run_speed: f32) -> f32 {
    let run_correction_time = rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER;
    let motion_speed_factor = (motion_speed / run_speed).clamp(0.0, 1.0);
    let correction_duration = RECON_PLAYER_IDLE_CORRECTION_SECS.lerp(run_correction_time, motion_speed_factor);
    (SNAPSHOT_SECS / correction_duration).clamp(0.0, 1.0)
}

// Per-axis snap distance, lerped from the idle to the running threshold by
// the recent-speed high-water mark (see `decayed_snap_speed`).
fn player_snap_threshold(snap_speed: f32, run_speed: f32) -> f32 {
    let threshold_speed_factor = (snap_speed / run_speed).clamp(0.0, 1.0);
    RECON_PLAYER_SNAP_DISTANCE_IDLE.lerp(RECON_PLAYER_SNAP_DISTANCE_RUNNING, threshold_speed_factor)
}

// High-water decay: a fresh, faster server speed wins immediately; otherwise
// the mark bleeds down over `RECON_PLAYER_SNAP_DECAY_SECS` so the snap
// threshold doesn't tighten abruptly on stop.
pub(super) fn decayed_snap_speed(previous: f32, server_speed: f32, run_speed: f32, delta: f32) -> f32 {
    let decay_step = run_speed / RECON_PLAYER_SNAP_DECAY_SECS * delta;
    (previous - decay_step).max(server_speed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN_SPEED: f32 = 8.0;

    #[test]
    fn idle_player_corrects_over_the_idle_window() {
        let factor = player_correction_factor(0.05, 0.0, RUN_SPEED);
        assert_eq!(factor, SNAPSHOT_SECS / RECON_PLAYER_IDLE_CORRECTION_SECS);
    }

    #[test]
    fn running_player_corrects_over_the_rtt_window() {
        let rtt = 0.2;
        let factor = player_correction_factor(rtt, RUN_SPEED, RUN_SPEED);
        let expected = SNAPSHOT_SECS / (rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER);
        assert!((factor - expected).abs() < 1e-6);
    }

    #[test]
    fn zero_rtt_saturates_the_correction_factor() {
        assert_eq!(player_correction_factor(0.0, RUN_SPEED, RUN_SPEED), 1.0);
    }

    #[test]
    fn snap_threshold_lerps_from_idle_to_running() {
        assert_eq!(player_snap_threshold(0.0, RUN_SPEED), RECON_PLAYER_SNAP_DISTANCE_IDLE);
        assert_eq!(
            player_snap_threshold(RUN_SPEED, RUN_SPEED),
            RECON_PLAYER_SNAP_DISTANCE_RUNNING
        );
        // Speed power-up can push server speed past run_speed; the clamp holds.
        assert_eq!(
            player_snap_threshold(2.0 * RUN_SPEED, RUN_SPEED),
            RECON_PLAYER_SNAP_DISTANCE_RUNNING
        );
    }

    #[test]
    fn snap_speed_decays_toward_the_current_server_speed() {
        let decayed = decayed_snap_speed(RUN_SPEED, 0.0, RUN_SPEED, 0.5);
        assert_eq!(decayed, RUN_SPEED - RUN_SPEED / RECON_PLAYER_SNAP_DECAY_SECS * 0.5);
    }

    #[test]
    fn faster_fresh_server_speed_wins_immediately() {
        assert_eq!(decayed_snap_speed(1.0, 6.0, RUN_SPEED, 0.1), 6.0);
    }

    #[test]
    fn snap_speed_never_falls_below_the_live_server_speed() {
        assert_eq!(decayed_snap_speed(0.0, 3.0, RUN_SPEED, 0.1), 3.0);
    }
}
