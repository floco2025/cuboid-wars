use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    constants::{
        ALWAYS_ANTI_GRAVITY, ALWAYS_PHASING, CHARACTER_GROUND_SNAP_DISTANCE, PHYSICS_EPSILON, UPDATE_BROADCAST_INTERVAL,
    },
    physics::{CharacterMovePlan, CollisionWorld, step_character_movement},
    protocol::Position,
};

use super::{feedback::decay_flash_timer, types::PlayerMovementQuery};
use crate::{network::ServerReconciliation, players::PlayerMap, ui::BumpFlashMarker};

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
        let is_standing_still = h_vel.x.hypot(h_vel.z) < PHYSICS_EPSILON;

        let mut target_pos = if let Some(recon) = recon_option.as_mut() {
            reconciled_target_position(
                commands,
                entity,
                &mut client_pos,
                &mut motion.0,
                recon,
                h_vel,
                is_local,
                is_standing_still,
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
    is_standing_still: bool,
    delta: f32,
    player_name: String,
    planned_moves: &mut Vec<CharacterMovePlan>,
    player_physics: common::config::CharacterPhysicsConfig,
    run_speed: f32,
) -> Option<Position> {
    // Slow idle correction reduces visible sliding when standing players receive
    // snapshot corrections. Moving characters hide correction better.
    const IDLE_RECONCILIATION_TIME: f32 = 10.0;
    let run_correction_time: f32 = recon.rtt * 5.0; // Benchmark: RTT = 100ms equals 0.5s correction time

    let speed_ratio = (h_vel.x.hypot(h_vel.z) / run_speed).clamp(0.0, 1.0);
    let correction_time_interval = IDLE_RECONCILIATION_TIME.lerp(run_correction_time, speed_ratio);
    let correction_factor = (UPDATE_BROADCAST_INTERVAL / correction_time_interval).clamp(0.0, 1.0);

    recon.timer += delta * correction_factor;
    if recon.timer >= UPDATE_BROADCAST_INTERVAL {
        commands.entity(entity).remove::<ServerReconciliation>();
    }

    let server_pos = Vec3::from(recon.server_pos) + recon.server_velocity * recon.rtt / 2.0;
    let total_delta = server_pos - Vec3::from(recon.client_pos);

    if is_local && total_delta.y.abs() >= CHARACTER_GROUND_SNAP_DISTANCE {
        *vertical_velocity = recon.server_velocity.y;
    }

    let out_of_sync_distance = if is_standing_still { 1.0 } else { 5.0 };
    if total_delta.x.abs() >= out_of_sync_distance
        || total_delta.y.abs() >= out_of_sync_distance
        || total_delta.z.abs() >= out_of_sync_distance
    {
        warn!(
            "{player_name} out of sync by x={:.2}, y={:.2}, z={:.2}; jumping to server position",
            total_delta.x, total_delta.y, total_delta.z
        );
        *client_pos = recon.server_pos;
        *vertical_velocity = recon.server_velocity.y;
        commands.entity(entity).remove::<ServerReconciliation>();
        planned_moves.push(CharacterMovePlan::stationary(
            entity,
            *client_pos,
            *vertical_velocity,
            player_physics,
        ));
        return None;
    }

    let dx = total_delta.x * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;
    let dz = total_delta.z * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;

    Some(Position {
        x: h_vel.x.mul_add(delta, client_pos.x) + dx,
        y: client_pos.y,
        z: h_vel.z.mul_add(delta, client_pos.z) + dz,
    })
}
