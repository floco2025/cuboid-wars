use bevy::prelude::*;
use common::{
    physics::CharacterVerticalVelocity,
    protocol::{PlayerId, Position},
};

use crate::{
    characters::PreviousTickPosition,
    constants::{
        RECON_CORRECTION_MIN_SECS, RECON_CORRECTION_TIME_RTT_MULTIPLIER, RECON_PLAYER_IDLE_CORRECTION_SECS,
        RECON_PLAYER_SNAP_DECAY_SECS, RECON_PLAYER_SNAP_DISTANCE_IDLE, RECON_PLAYER_SNAP_DISTANCE_RUNNING,
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
    let window = player_correction_window(recon.rtt, motion_speed, run_speed);

    // During move-stream pauses, speed can change the window, so completion follows the applied fraction.
    let fraction = (delta / window).min(1.0 - recon.applied_fraction);
    recon.applied_fraction += fraction;
    if recon.applied_fraction >= 1.0 {
        commands.entity(entity).remove::<ServerReconciliation>();
    }

    let correction_delta = recon.correction_delta;

    // Y is purely predicted: client owns jump intent and gravity, so vy is
    // locally authoritative. The snap branch still catches big disagreements
    // (e.g. landed-on-different-floor). Sub-threshold Y divergence persists
    // while moving (the threshold lerps up to the running distance) but
    // heals on stop: `snap_speed` decays back within
    // ~`RECON_PLAYER_SNAP_DECAY_SECS`, tightening the threshold to the idle
    // distance, so a floor-level disagreement snaps once the player
    // settles. Smoothing Y instead is not an option — the physics step only
    // takes the target's X/Z, and a nudged Y would be pushed back onto the
    // client-side floor by ground snapping.
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
        correction_delta.x * fraction,
        0.0,
        correction_delta.z * fraction,
    ))
}

// The correction window lerps from the idle constant (long and gentle:
// stationary players see corrections clearly) to the RTT-scaled running
// window (short: motion hides the drift) by how fast the player is moving
// right now, and never drops below `RECON_CORRECTION_MIN_SECS`.
fn player_correction_window(rtt: f32, motion_speed: f32, run_speed: f32) -> f32 {
    let run_correction_time = rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER;
    let motion_speed_factor = (motion_speed / run_speed).clamp(0.0, 1.0);
    RECON_PLAYER_IDLE_CORRECTION_SECS
        .lerp(run_correction_time, motion_speed_factor)
        .max(RECON_CORRECTION_MIN_SECS)
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
    use crate::network::RoundTripTime;
    use bevy::ecs::system::SystemState;

    const RUN_SPEED: f32 = 8.0;

    fn assert_paused_correction_finishes(initial_speed: f32, change_tick: usize, final_speed: f32) {
        let mut world = World::new();
        let correction = Vec3::new(0.5, 0.2, -0.3);
        let entity = world
            .spawn((
                Position::default(),
                CharacterVerticalVelocity(0.0),
                ServerReconciliation::new(correction, correction.into(), Vec3::ZERO, &RoundTripTime::default()),
            ))
            .id();
        let mut state = SystemState::<(
            Commands,
            Query<(&mut Position, &mut CharacterVerticalVelocity, &mut ServerReconciliation)>,
        )>::new(&mut world);

        for tick in 0..300 {
            if world.get::<ServerReconciliation>(entity).is_none() {
                break;
            }
            let speed = if tick < change_tick { initial_speed } else { final_speed };
            let (mut commands, mut query) = state
                .get_mut(&mut world)
                .expect("reconciliation system state is invalid");
            let (mut position, mut vertical_velocity, mut recon) = query.single_mut().expect("player missing");
            let outcome = reconcile_player(
                &mut commands,
                entity,
                &PlayerId(1),
                None,
                &mut position,
                &mut vertical_velocity,
                &mut recon,
                Vec3::X * speed,
                1.0 / 30.0,
                RUN_SPEED,
                speed,
            );
            let PlayerReconciliationOutcome::Displacement(displacement) = outcome else {
                panic!("small correction snapped");
            };
            *position += displacement;
            assert!(position.x <= correction.x + 1e-6, "correction overshot on tick {tick}");
            state.apply(&mut world);
        }

        assert!(
            world.get::<ServerReconciliation>(entity).is_none(),
            "correction did not finish"
        );
        let position = world.get::<Position>(entity).expect("player position missing");
        assert!(
            (position.x - correction.x).abs() < 1e-6,
            "X correction incomplete: {position:?}"
        );
        assert!(
            (position.z - correction.z).abs() < 1e-6,
            "Z correction incomplete: {position:?}"
        );
        assert_eq!(position.y, 0.0);
    }

    #[test]
    fn paused_movement_stream_finishes_correction_when_starting_to_run() {
        assert_paused_correction_finishes(0.0, 9, RUN_SPEED);
    }

    #[test]
    fn paused_movement_stream_finishes_correction_when_stopping() {
        assert_paused_correction_finishes(RUN_SPEED, 3, 0.0);
    }

    #[test]
    fn paused_movement_stream_caps_the_last_correction_step() {
        assert_paused_correction_finishes(RUN_SPEED, 0, RUN_SPEED);
    }

    #[test]
    fn idle_player_corrects_over_the_idle_window() {
        assert_eq!(
            player_correction_window(0.05, 0.0, RUN_SPEED),
            RECON_PLAYER_IDLE_CORRECTION_SECS
        );
    }

    #[test]
    fn running_player_corrects_over_the_rtt_window() {
        let rtt = 0.2;
        let window = player_correction_window(rtt, RUN_SPEED, RUN_SPEED);
        let expected = rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER;
        assert!((window - expected).abs() < 1e-6);
    }

    #[test]
    fn zero_rtt_keeps_the_minimum_window() {
        assert_eq!(
            player_correction_window(0.0, RUN_SPEED, RUN_SPEED),
            RECON_CORRECTION_MIN_SECS
        );
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
