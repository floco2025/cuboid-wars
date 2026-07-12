use bevy::prelude::*;

use crate::{
    actors::ActorMap,
    characters::PreviousTickPosition,
    constants::{RECON_ACTOR_SNAP_DISTANCE, RECON_CORRECTION_TIME_RTT_MULTIPLIER},
    network::{ServerReconciliation, worst_axis_divergence},
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::SNAPSHOT_SECS,
    physics::{
        CharacterMovePlan, CharacterVerticalVelocity, CollisionWorld, OpenBarrierKinds, blocking_character_move_plan,
        character_move_plan_is_blocked, step_character_movement,
    },
    protocol::{ActorId, ActorMarker, ActorMoveIntent, MapSettings, PlayerMarker, Position},
};

// Fraction of the correction delta applied per `SNAPSHOT_SECS` of real time
// — `SNAPSHOT_SECS / correction window`, with the window scaled from the
// RTT. A near-zero RTT saturates to 1.0 via the clamp. Unlike players,
// actors get no motion-aware window: their speeds are simple enough that
// one RTT-scaled window fits.
fn actor_correction_factor(rtt: f32) -> f32 {
    (SNAPSHOT_SECS / (rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER)).clamp(0.0, 1.0)
}

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

pub(crate) fn actor_start_positions(
    query: &ActorMovementQuery,
    actors: &ActorMap,
    gameplay_config: &GameplayConfig,
) -> Vec<(Entity, Position, CharacterPhysicsConfig)> {
    query
        .iter()
        .filter_map(|(entity, actor_id, pos, _, _, _)| {
            let info = actors.get(actor_id)?;
            let physics = gameplay_config
                .actor(&info.kind)
                .expect("actor kind sent by server is missing from gameplay config")
                .physics();
            Some((entity, *pos, physics))
        })
        .collect()
}

pub(crate) fn plan_actor_moves(
    commands: &mut Commands,
    delta: f32,
    collision_world: Option<&CollisionWorld>,
    map_settings: Option<&MapSettings>,
    gameplay_config: &GameplayConfig,
    actors: &ActorMap,
    open_barrier_kinds: &OpenBarrierKinds,
    actor_starts: &[(Entity, Position, CharacterPhysicsConfig)],
    query: &mut ActorMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    for (entity, actor_id, mut pos, move_intent, mut motion, mut recon_option) in query {
        let Some(info) = actors.get(actor_id) else {
            continue;
        };
        let actor_physics = gameplay_config
            .actor(&info.kind)
            .expect("actor kind sent by server is missing from gameplay config")
            .physics();
        let h_vel = move_intent.to_horizontal_velocity();
        let target_pos = if let Some(recon) = recon_option.as_mut() {
            let correction_factor = actor_correction_factor(recon.rtt);

            // Each tick applies `delta / correction window` of the fixed
            // delta, so the accumulator reaching `SNAPSHOT_SECS` coincides
            // with exactly 100% of the correction applied — removing the
            // component here is what stops over-correction, doubling as the
            // dropped-snapshot fallback (normally the next snapshot replaces
            // this component first).
            recon.correction_progress += delta * correction_factor;
            if recon.correction_progress >= SNAPSHOT_SECS {
                commands.entity(entity).remove::<ServerReconciliation>();
            }

            let correction_delta = recon.extrapolated_delta();

            let (worst_axis, worst_magnitude) = worst_axis_divergence(correction_delta);
            if worst_magnitude >= RECON_ACTOR_SNAP_DISTANCE {
                warn!(
                    "{:?} out of sync: |{worst_axis}|={worst_magnitude:.2} >= {:.2} (Δ x={:.2}, y={:.2}, z={:.2}); snapping to server position",
                    actor_id, RECON_ACTOR_SNAP_DISTANCE, correction_delta.x, correction_delta.y, correction_delta.z
                );
                *pos = recon.server_pos;
                // Adopt server vy only; horizontal motion comes from `move_intent`.
                motion.0 = recon.server_velocity.y;
                commands.entity(entity).remove::<ServerReconciliation>();
                // Anchor render interpolation at the snapped position so the
                // snap doesn't smear across one render frame.
                commands.entity(entity).insert(PreviousTickPosition(*pos));
                push_actor_planned_move(
                    planned_moves,
                    actor_starts,
                    CharacterMovePlan::stationary(entity, *pos, motion.0, actor_physics),
                );
                continue;
            }

            Position {
                x: h_vel.x.mul_add(delta, pos.x) + correction_delta.x * delta * correction_factor / SNAPSHOT_SECS,
                // Y is purely predicted (gravity runs locally); the snap
                // branch above catches floor-level disagreements.
                y: pos.y,
                z: h_vel.z.mul_add(delta, pos.z) + correction_delta.z * delta * correction_factor / SNAPSHOT_SECS,
            }
        } else {
            Position {
                x: h_vel.x.mul_add(delta, pos.x),
                y: pos.y,
                z: h_vel.z.mul_add(delta, pos.z),
            }
        };

        // `CollisionWorld` and `MapSettings` are both installed by the same
        // `SInit`, so they appear together.
        if let (Some(collision_world), Some(map_settings)) = (collision_world, map_settings) {
            let step = step_character_movement(
                &pos,
                motion.0,
                collision_world,
                false,
                map_settings.gravity, // actors never have low-gravity
                &open_barrier_kinds.0,
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
    actor_starts: &[(Entity, Position, CharacterPhysicsConfig)],
    mut planned_move: CharacterMovePlan,
) {
    if character_move_plan_is_blocked(&planned_move, planned_moves, actor_starts) {
        planned_move = planned_move.with_blocked_xz();
    }
    planned_moves.push(planned_move);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_factor_follows_the_rtt_window() {
        let rtt = 0.2;
        let expected = SNAPSHOT_SECS / (rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER);
        assert!((actor_correction_factor(rtt) - expected).abs() < 1e-6);
    }

    #[test]
    fn zero_rtt_saturates_the_correction_factor() {
        assert_eq!(actor_correction_factor(0.0), 1.0);
    }
}
