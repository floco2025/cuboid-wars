use bevy::prelude::*;

use crate::{
    actors::ActorMap,
    constants::{RECON_ACTOR_OUT_OF_SYNC_DISTANCE, RECON_RTT_MULTIPLIER},
    network::ServerReconciliation,
};
use common::{
    config::GameplayConfig,
    constants::UPDATE_BROADCAST_INTERVAL,
    physics::{
        CharacterMovePlan, CharacterVerticalVelocity, CollisionWorld, blocking_character_move_plan,
        character_move_plan_is_blocked, step_character_movement,
    },
    protocol::{ActorId, ActorMarker, ActorMoveIntent, PlayerMarker, Position},
};

pub(crate) type ActorMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ActorId,
        &'static mut Position,
        &'static ActorMoveIntent,
        &'static mut CharacterVerticalVelocity,
        Option<&'static mut ServerReconciliation>,
    ),
    (With<ActorMarker>, Without<PlayerMarker>),
>;

pub(crate) fn actor_start_positions(query: &ActorMovementQuery) -> Vec<(Entity, Position)> {
    query.iter().map(|(entity, _, pos, _, _, _)| (entity, *pos)).collect()
}

pub(crate) fn plan_actor_moves(
    commands: &mut Commands,
    delta: f32,
    collision_world: Option<&CollisionWorld>,
    gameplay_config: &GameplayConfig,
    actors: &ActorMap,
    actor_starts: &[(Entity, Position)],
    query: &mut ActorMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    for (entity, actor_id, mut pos, move_intent, mut motion, mut recon_option) in query {
        let Some(info) = actors.get(actor_id) else {
            continue;
        };
        let actor_physics = gameplay_config
            .actor(&info.kind)
            .expect("actor kind sent by server is in gameplay config")
            .physics();
        let h_vel = move_intent.to_horizontal_velocity();
        let target_pos = if let Some(recon) = recon_option.as_mut() {
            let correction_time = recon.rtt * RECON_RTT_MULTIPLIER;
            let correction_factor = (UPDATE_BROADCAST_INTERVAL / correction_time).clamp(0.0, 1.0);

            recon.timer += delta * correction_factor;
            if recon.timer >= UPDATE_BROADCAST_INTERVAL {
                commands.entity(entity).remove::<ServerReconciliation>();
            }

            let server_pos = Vec3::from(recon.server_pos) + recon.server_velocity * recon.rtt / 2.0;
            let total_delta = server_pos - Vec3::from(recon.client_pos);

            if total_delta.x.abs() >= RECON_ACTOR_OUT_OF_SYNC_DISTANCE
                || total_delta.y.abs() >= RECON_ACTOR_OUT_OF_SYNC_DISTANCE
                || total_delta.z.abs() >= RECON_ACTOR_OUT_OF_SYNC_DISTANCE
            {
                warn!(
                    "{:?} out of sync by x={:.2}, y={:.2}, z={:.2}; jumping to server position",
                    actor_id, total_delta.x, total_delta.y, total_delta.z
                );
                *pos = recon.server_pos;
                motion.0 = recon.server_velocity.y;
                commands.entity(entity).remove::<ServerReconciliation>();
                push_actor_planned_move(
                    planned_moves,
                    actor_starts,
                    CharacterMovePlan::stationary(entity, *pos, motion.0, actor_physics),
                );
                continue;
            }

            Position {
                x: h_vel.x.mul_add(delta, pos.x)
                    + total_delta.x * delta * correction_factor / UPDATE_BROADCAST_INTERVAL,
                y: pos.y,
                z: h_vel.z.mul_add(delta, pos.z)
                    + total_delta.z * delta * correction_factor / UPDATE_BROADCAST_INTERVAL,
            }
        } else {
            Position {
                x: h_vel.x.mul_add(delta, pos.x),
                y: pos.y,
                z: h_vel.z.mul_add(delta, pos.z),
            }
        };

        if let Some(collision_world) = collision_world {
            let step = step_character_movement(
                &pos,
                motion.0,
                collision_world,
                false,
                false, // actors never have anti-gravity
                &[],   // actors never hold barrier keys
                actor_physics,
                target_pos.x,
                target_pos.z,
                delta,
            );
            push_actor_planned_move(
                planned_moves,
                actor_starts,
                CharacterMovePlan::from_movement_result(entity, *pos, step, actor_physics),
            );
        } else {
            push_actor_planned_move(
                planned_moves,
                actor_starts,
                CharacterMovePlan::from_target(entity, *pos, target_pos, motion.0, actor_physics, false),
            );
        }
    }
}

pub(crate) fn apply_actor_moves(query: &mut ActorMovementQuery, planned_moves: &[CharacterMovePlan]) {
    for planned_move in planned_moves {
        let Ok((_, _, mut pos, _, mut motion, _)) = query.get_mut(planned_move.entity) else {
            continue;
        };

        if blocking_character_move_plan(planned_move, planned_moves).is_some() {
            pos.y = planned_move.target.y;
        } else {
            *pos = planned_move.target;
        }
        motion.0 = planned_move.target_vertical_velocity;
    }
}

fn push_actor_planned_move(
    planned_moves: &mut Vec<CharacterMovePlan>,
    actor_starts: &[(Entity, Position)],
    mut planned_move: CharacterMovePlan,
) {
    if character_move_plan_is_blocked(&planned_move, planned_moves, actor_starts) {
        planned_move = planned_move.with_blocked_xz();
    }
    planned_moves.push(planned_move);
}
