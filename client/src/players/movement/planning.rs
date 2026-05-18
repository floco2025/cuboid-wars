use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    constants::{ALWAYS_ANTI_GRAVITY, ALWAYS_PHASING, SNAPSHOT_SECS},
    physics::{CharacterMovePlan, CollisionWorld, step_character_movement},
    protocol::{PlayerId, Position},
};

use super::types::PlayerMovementQuery;
use crate::{
    characters::PreviousTickPosition,
    constants::{
        RECON_CORRECTION_TIME_RTT_MULTIPLIER, RECON_PLAYER_IDLE_CORRECTION_SECS, RECON_PLAYER_SNAP_DECAY_SECS,
        RECON_PLAYER_SNAP_DISTANCE_IDLE, RECON_PLAYER_SNAP_DISTANCE_RUNNING,
    },
    network::{ServerReconciliation, worst_axis_excess},
    players::PlayerMap,
};

pub(crate) fn plan_player_moves(
    commands: &mut Commands,
    delta: f32,
    collision_world: Option<&CollisionWorld>,
    gameplay_config: &GameplayConfig,
    players: &mut PlayerMap,
    query: &mut PlayerMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let player_config = &gameplay_config.player;
    let player_physics = player_config.physics();
    for (entity, player_id, mut client_pos, move_intent, mut motion, _, mut recon_option, _) in query {
        // Decay snap_speed each tick; new snapshot speed wins if larger.
        // Persisted on `PlayerInfo`; see `RECON_PLAYER_SNAP_DECAY_SECS`
        // for the why.
        let current_server_speed = recon_option.as_ref().map_or(0.0, |r| r.server_velocity.xz().length());
        let snap_speed = match players.get_mut(player_id) {
            Some(info) => {
                let decay_step = player_config.run_speed / RECON_PLAYER_SNAP_DECAY_SECS * delta;
                info.snap_speed = (info.snap_speed - decay_step).max(current_server_speed);
                info.snap_speed
            }
            None => current_server_speed,
        };

        // Immutable lookup for the read-only fields; the mut borrow above ended with the `match`.
        let info = players.get(player_id);
        let has_speed_power_up = info.is_some_and(|i| i.speed_power_up);
        let has_phasing = ALWAYS_PHASING || info.is_some_and(|i| i.phasing_power_up);
        let has_anti_gravity = ALWAYS_ANTI_GRAVITY || info.is_some_and(|i| i.anti_gravity_power_up);
        let held_keys: &[common::protocol::BarrierKindId] = info.map_or(&[], |i| i.held_keys.as_slice());
        let player_name = info.map(|i| i.name.as_str());

        let h_vel =
            move_intent.to_horizontal_velocity(player_config.walk_speed, player_config.run_speed, has_speed_power_up);

        let mut target_pos = if let Some(recon) = recon_option.as_mut() {
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

        if target_pos.is_none() {
            continue;
        }
        let target = target_pos
            .take()
            .expect("target_pos is present after out-of-sync shortcut");

        if let Some(collision_world) = collision_world {
            let step = step_character_movement(
                &client_pos,
                motion.0,
                collision_world,
                has_phasing,
                has_anti_gravity,
                held_keys,
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

#[allow(clippy::too_many_arguments)]
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
    let run_correction_time = recon.rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER;
    // Motion-aware lerp between idle (long, gentle) and running (short)
    // windows. Vertical velocity counts — a jumping or falling player
    // with no horizontal input is still in motion.
    let motion_speed = h_vel.x.hypot(h_vel.z).hypot(*vertical_velocity);
    let motion_speed_factor = (motion_speed / run_speed).clamp(0.0, 1.0);
    let correction_duration = RECON_PLAYER_IDLE_CORRECTION_SECS.lerp(run_correction_time, motion_speed_factor);
    let correction_factor = (SNAPSHOT_SECS / correction_duration).clamp(0.0, 1.0);

    // Accumulator hits `SNAPSHOT_PERIOD_SECS` after exactly
    // `correction_duration` real seconds. Usually the next snapshot
    // overwrites this component first; the accumulator is the
    // dropped-snapshot fallback.
    recon.correction_progress += delta * correction_factor;
    if recon.correction_progress >= SNAPSHOT_SECS {
        commands.entity(entity).remove::<ServerReconciliation>();
    }

    // Project the snapshot pos forward by half-RTT — compare against
    // where the server is *now*, not where it was at snapshot time.
    let extrapolated_server_pos = Vec3::from(recon.server_pos) + recon.server_velocity * recon.rtt / 2.0;
    let correction_delta = extrapolated_server_pos - Vec3::from(recon.client_pos);

    // Y is purely predicted: client owns jump intent and gravity, so
    // vy is locally authoritative. The snap branch still catches big
    // disagreements (e.g. landed-on-different-floor) via the per-axis
    // `worst_axis_excess` check on `correction_delta`.

    let threshold_speed_factor = (snap_speed / run_speed).clamp(0.0, 1.0);
    let snap_threshold =
        RECON_PLAYER_SNAP_DISTANCE_IDLE.lerp(RECON_PLAYER_SNAP_DISTANCE_RUNNING, threshold_speed_factor);
    let (worst_axis, worst_magnitude) = worst_axis_excess(correction_delta);
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
