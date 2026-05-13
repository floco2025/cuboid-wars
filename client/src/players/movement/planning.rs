use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    constants::{ALWAYS_ANTI_GRAVITY, ALWAYS_PHASING, CHARACTER_GROUND_SNAP_DISTANCE, SNAPSHOT_PERIOD_SECS},
    physics::{CharacterMovePlan, CollisionWorld, step_character_movement},
    protocol::Position,
};

use super::{feedback::decay_flash_timer, types::PlayerMovementQuery};
use crate::{
    characters::PreviousTickPosition,
    constants::{
        RECON_CORRECTION_TIME_RTT_MULTIPLIER, RECON_PLAYER_IDLE_CORRECTION_TIME, RECON_PLAYER_SNAP_THRESHOLD_IDLE,
        RECON_PLAYER_SNAP_THRESHOLD_RUNNING,
    },
    network::{ServerReconciliation, worst_axis_excess},
    players::PlayerMap,
    ui::BumpFlashMarker,
};

pub(crate) fn plan_player_moves(
    commands: &mut Commands,
    delta: f32,
    collision_world: Option<&CollisionWorld>,
    gameplay_config: &GameplayConfig,
    players: &PlayerMap,
    query: &mut PlayerMovementQuery,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashMarker>>,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let player_config = &gameplay_config.player;
    let player_physics = player_config.physics();
    for (entity, player_id, mut client_pos, move_intent, mut motion, mut flash_state, mut recon_option, is_local) in
        query
    {
        if let Some(state) = flash_state.as_mut() {
            decay_flash_timer(state, delta, is_local, bump_flash_ui);
        }

        let has_speed_power_up = players.get(player_id).is_some_and(|info| info.speed_power_up);
        let h_vel =
            move_intent.to_horizontal_velocity(player_config.walk_speed, player_config.run_speed, has_speed_power_up);

        let mut target_pos = if let Some(recon) = recon_option.as_mut() {
            reconciled_target_position(
                commands,
                entity,
                &mut client_pos,
                &mut motion.0,
                recon,
                h_vel,
                is_local,
                delta,
                players
                    .get(player_id)
                    .map_or_else(|| format!("{player_id:?}"), |info| info.name.clone()),
                planned_moves,
                player_physics,
                player_config.run_speed,
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
            let has_phasing = ALWAYS_PHASING || players.get(player_id).is_some_and(|info| info.phasing_power_up);
            let has_anti_gravity =
                ALWAYS_ANTI_GRAVITY || players.get(player_id).is_some_and(|info| info.anti_gravity_power_up);

            let held_keys: &[common::protocol::BarrierKindId] =
                players.get(player_id).map_or(&[], |info| info.held_keys.as_slice());
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
    client_pos: &mut Position,
    vertical_velocity: &mut f32,
    recon: &mut ServerReconciliation,
    h_vel: Vec3,
    is_local: bool,
    delta: f32,
    player_name: String,
    planned_moves: &mut Vec<CharacterMovePlan>,
    player_physics: common::config::CharacterPhysicsConfig,
    run_speed: f32,
) -> Option<Position> {
    let run_correction_time = recon.rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER;
    // Correction duration follows current input (not server_velocity)
    // so the duration lengthens the instant the player stops, keeping
    // any residual correction below the visible-distraction threshold.
    let input_speed_factor = (h_vel.x.hypot(h_vel.z) / run_speed).clamp(0.0, 1.0);
    let correction_duration = RECON_PLAYER_IDLE_CORRECTION_TIME.lerp(run_correction_time, input_speed_factor);
    let correction_factor = (SNAPSHOT_PERIOD_SECS / correction_duration).clamp(0.0, 1.0);

    // Accumulator reaches `SNAPSHOT_PERIOD_SECS` after exactly
    // `correction_duration` real seconds. The next snapshot normally
    // overwrites this component before then; the accumulator is the
    // fallback when snapshots are dropped.
    recon.correction_progress += delta * correction_factor;
    if recon.correction_progress >= SNAPSHOT_PERIOD_SECS {
        commands.entity(entity).remove::<ServerReconciliation>();
    }

    // Project the snapshot's server pos forward by half-RTT so we compare
    // against where the server is *now*, not where it was when the
    // snapshot was sent.
    let extrapolated_server_pos = Vec3::from(recon.server_pos) + recon.server_velocity * recon.rtt / 2.0;
    let correction_delta = extrapolated_server_pos - Vec3::from(recon.client_pos);

    // Local player's vertical velocity is authoritative from the server
    // (jumps + gravity); adopt it eagerly when the recon disagrees by
    // more than the ground-snap tolerance. Remote players derive vy from
    // their own predicted physics, so the gradual horizontal correction
    // below is enough.
    if is_local && correction_delta.y.abs() >= CHARACTER_GROUND_SNAP_DISTANCE {
        *vertical_velocity = recon.server_velocity.y;
    }

    // Threshold uses the snapshot-captured server velocity, not current
    // input. `recon.server_velocity` stays put until the next snapshot
    // overwrites it, so a drift captured while running keeps the running
    // threshold for that ~one-snapshot window — releasing the run key on
    // the next tick can't tighten the threshold mid-correction.
    let server_speed = recon.server_velocity.xz().length();
    let threshold_speed_factor = (server_speed / run_speed).clamp(0.0, 1.0);
    let snap_threshold =
        RECON_PLAYER_SNAP_THRESHOLD_IDLE.lerp(RECON_PLAYER_SNAP_THRESHOLD_RUNNING, threshold_speed_factor);
    let (worst_axis, worst_magnitude) = worst_axis_excess(correction_delta);
    if worst_magnitude >= snap_threshold {
        warn!(
            "{player_name} out of sync: |{worst_axis}|={worst_magnitude:.2} >= {snap_threshold:.2} (Δ x={:.2}, y={:.2}, z={:.2}); snapping to server position",
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

    let dx = correction_delta.x * delta * correction_factor / SNAPSHOT_PERIOD_SECS;
    let dz = correction_delta.z * delta * correction_factor / SNAPSHOT_PERIOD_SECS;

    Some(Position {
        x: h_vel.x.mul_add(delta, client_pos.x) + dx,
        y: client_pos.y,
        z: h_vel.z.mul_add(delta, client_pos.z) + dz,
    })
}
