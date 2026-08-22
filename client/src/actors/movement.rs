use bevy::prelude::*;

use crate::{actors::ActorMap, network::ServerReconciliation};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    physics::{
        CharacterEnvironment, CharacterMovePlan, CharacterStep, CharacterVerticalVelocity, CollisionWorld,
        OpenBarrierKinds, blocking_character_move_plan, character_move_plan_is_blocked, step_character_movement,
    },
    protocol::{ActorId, ActorMarker, ActorMoveIntent, MapSettings, PlayerMarker, Position},
};

use super::reconciliation::{ActorReconciliationOutcome, reconcile_actor};

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
        let control_velocity = move_intent.to_horizontal_velocity();
        let correction_displacement = match recon_option.as_mut() {
            Some(recon) => match reconcile_actor(
                commands,
                entity,
                actor_id,
                &info.kind,
                &mut pos,
                &mut motion,
                recon,
                delta,
            ) {
                ActorReconciliationOutcome::Displacement(displacement) => displacement,
                ActorReconciliationOutcome::Snapped => {
                    push_actor_planned_move(
                        planned_moves,
                        actor_starts,
                        CharacterMovePlan::stationary(entity, *pos, motion.0, actor_physics),
                    );
                    continue;
                }
            },
            None => Vec3::ZERO,
        };

        // `CollisionWorld` and `MapSettings` are both installed by the same
        // `SInit`, so they appear together.
        if let (Some(collision_world), Some(map_settings)) = (collision_world, map_settings) {
            let step = step_character_movement(
                CharacterStep {
                    start: *pos,
                    vertical_velocity: motion.0,
                    control_velocity,
                    external_displacement: correction_displacement,
                    delta,
                },
                &CharacterEnvironment {
                    collision_world,
                    gravity: map_settings.gravity,
                    passable_kinds: &open_barrier_kinds.0,
                    physics: actor_physics,
                    ladders: gameplay_config.ladders,
                },
            );
            push_actor_planned_move(
                planned_moves,
                actor_starts,
                CharacterMovePlan::from_movement_result(entity, *pos, step, actor_physics),
            );
        } else {
            let target_pos = Position {
                x: control_velocity.x.mul_add(delta, pos.x) + correction_displacement.x,
                y: pos.y,
                z: control_velocity.z.mul_add(delta, pos.z) + correction_displacement.z,
            };
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
