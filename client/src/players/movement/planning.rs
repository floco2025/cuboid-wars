use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    constants::{ALWAYS_ANTI_GRAVITY, ALWAYS_PHASING, SNAPSHOT_SECS},
    physics::{CharacterMovePlan, CollisionWorld, passable_barrier_kinds, step_character_movement},
    protocol::{BarrierKindId, MapSettings, PlayerId, Position, PowerUpKind},
};

use super::types::PlayerMovementQuery;
use crate::{
    characters::PreviousTickPosition,
    constants::{
        RECON_CORRECTION_TIME_RTT_MULTIPLIER, RECON_PLAYER_IDLE_CORRECTION_SECS, RECON_PLAYER_SNAP_DECAY_SECS,
        RECON_PLAYER_SNAP_DISTANCE_IDLE, RECON_PLAYER_SNAP_DISTANCE_RUNNING,
    },
    network::{ServerReconciliation, worst_axis_divergence},
    players::PlayerMap,
};

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
fn decayed_snap_speed(previous: f32, server_speed: f32, run_speed: f32, delta: f32) -> f32 {
    let decay_step = run_speed / RECON_PLAYER_SNAP_DECAY_SECS * delta;
    (previous - decay_step).max(server_speed)
}

pub(crate) fn plan_player_moves(
    commands: &mut Commands,
    delta: f32,
    collision_world: Option<&CollisionWorld>,
    map_settings: Option<&MapSettings>,
    gameplay_config: &GameplayConfig,
    players: &mut PlayerMap,
    open_barrier_kinds: &crate::barriers::OpenBarrierKinds,
    query: &mut PlayerMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let player_config = &gameplay_config.player;
    let player_physics = player_config.physics();
    for (entity, player_id, mut client_pos, move_intent, mut motion, _, mut recon_option, _) in query {
        // Decay snap_speed each tick; new snapshot speed wins if larger.
        // Persisted on `PlayerInfo`. Deliberately fed by the SERVER velocity
        // (authoritative recent speed for the snap threshold), while the
        // correction window reads the predicted velocity — what the player
        // perceives right now.
        let current_server_speed = recon_option.as_ref().map_or(0.0, |r| r.server_velocity.xz().length());
        let snap_speed = match players.get_mut(player_id) {
            Some(info) => {
                info.snap_speed =
                    decayed_snap_speed(info.snap_speed, current_server_speed, player_config.run_speed, delta);
                info.snap_speed
            }
            None => current_server_speed,
        };

        // Immutable lookup for the read-only fields; the mut borrow above ended with the `match`.
        let info = players.get(player_id);
        let has_speed_power_up = info.is_some_and(|i| i.power_up(PowerUpKind::Speed));
        let has_phasing = ALWAYS_PHASING || info.is_some_and(|i| i.power_up(PowerUpKind::Phasing));
        let has_anti_gravity = ALWAYS_ANTI_GRAVITY || info.is_some_and(|i| i.power_up(PowerUpKind::AntiGravity));
        let held_keys: &[BarrierKindId] = info.map_or(&[], |i| i.held_keys.as_slice());
        let player_name = info.map(|i| i.name.as_str());

        let h_vel =
            move_intent.to_horizontal_velocity(player_config.walk_speed, player_config.run_speed, has_speed_power_up);

        let target_pos = if let Some(recon) = recon_option.as_mut() {
            reconciled_target_position(
                commands,
                entity,
                player_id,
                player_name,
                &mut client_pos,
                &mut motion.0,
                recon,
                h_vel,
                delta,
                planned_moves,
                player_physics,
                player_config.run_speed,
                snap_speed,
            )
        } else {
            Some(Position {
                x: h_vel.x.mul_add(delta, client_pos.x),
                y: client_pos.y,
                z: h_vel.z.mul_add(delta, client_pos.z),
            })
        };

        let Some(target) = target_pos else {
            continue;
        };

        // `CollisionWorld` and `MapSettings` are both installed by the same
        // `SInit`, so they appear together.
        if let (Some(collision_world), Some(map_settings)) = (collision_world, map_settings) {
            // Same merge the server runs (`passable_barrier_kinds`) so
            // client-side prediction agrees with server-authoritative
            // movement about which barriers we pass through.
            let passable_kinds = passable_barrier_kinds(held_keys, &open_barrier_kinds.0);
            let step = step_character_movement(
                &client_pos,
                motion.0,
                collision_world,
                has_phasing,
                map_settings.gravity_for(has_anti_gravity),
                &passable_kinds,
                player_physics,
                target.x,
                target.z,
                delta,
            );
            planned_moves.push(CharacterMovePlan::from_movement_result(
                entity,
                *client_pos,
                step,
                player_physics,
            ));
        } else {
            planned_moves.push(CharacterMovePlan::from_target(
                entity,
                *client_pos,
                target,
                motion.0,
                player_physics,
                false,
            ));
        }
    }
}

fn reconciled_target_position(
    commands: &mut Commands,
    entity: Entity,
    player_id: &PlayerId,
    player_name: Option<&str>,
    client_pos: &mut Position,
    vertical_velocity: &mut f32,
    recon: &mut ServerReconciliation,
    h_vel: Vec3,
    delta: f32,
    planned_moves: &mut Vec<CharacterMovePlan>,
    player_physics: common::config::CharacterPhysicsConfig,
    run_speed: f32,
    snap_speed: f32,
) -> Option<Position> {
    // Vertical velocity counts toward motion — a jumping or falling player
    // with no horizontal input is still in motion.
    let motion_speed = h_vel.x.hypot(h_vel.z).hypot(*vertical_velocity);
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
        *vertical_velocity = recon.server_velocity.y;
        commands.entity(entity).remove::<ServerReconciliation>();
        // Reset the prev-tick anchor so the next render frame doesn't lerp
        // through the snap.
        commands.entity(entity).insert(PreviousTickPosition(*client_pos));
        planned_moves.push(CharacterMovePlan::stationary(
            entity,
            *client_pos,
            *vertical_velocity,
            player_physics,
        ));
        return None;
    }

    let dx = correction_delta.x * delta * correction_factor / SNAPSHOT_SECS;
    let dz = correction_delta.z * delta * correction_factor / SNAPSHOT_SECS;

    Some(Position {
        x: h_vel.x.mul_add(delta, client_pos.x) + dx,
        y: client_pos.y,
        z: h_vel.z.mul_add(delta, client_pos.z) + dz,
    })
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
