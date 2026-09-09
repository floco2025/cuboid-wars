use bevy::prelude::*;

use crate::{
    actors::ActorMap,
    characters::{CharacterReconciliationOutcome, reconcile_character},
    network::ServerReconciliation,
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    map::Carriers,
    physics::{
        ActorMovementStep, CharacterMovePlan, CharacterVerticalVelocity, CollisionWorld, blocking_character_move_plan,
        character_move_plan_is_blocked, grounding_diagnostics, step_actor_movement,
    },
    protocol::{ActorId, ActorMarker, ActorMoveIntent, MapSettings, PlateState, PlayerMarker, Position},
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

pub(crate) fn actor_start_positions(
    query: &ActorMovementQuery,
    actors: &ActorMap,
    gameplay_config: &GameplayConfig,
) -> Vec<(Entity, Position, CharacterPhysicsConfig)> {
    query
        .iter()
        .filter_map(|(entity, actor_id, pos, _, _, _)| {
            let info = actors.get(actor_id)?;
            Some((entity, *pos, gameplay_config.expect_actor(&info.kind).physics()))
        })
        .collect()
}

pub(crate) fn plan_actor_moves(
    commands: &mut Commands,
    delta: f32,
    collision_world: &CollisionWorld,
    map_settings: &MapSettings,
    gameplay_config: &GameplayConfig,
    actors: &ActorMap,
    plates: &PlateState,
    carriers: &Carriers,
    actor_starts: &[(Entity, Position, CharacterPhysicsConfig)],
    query: &mut ActorMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    for (entity, actor_id, mut pos, move_intent, mut motion, mut recon_option) in query {
        let Some(info) = actors.get(actor_id) else {
            continue;
        };
        let actor_config = gameplay_config.expect_actor(&info.kind);
        let actor_physics = actor_config.physics();
        if let Some(anchor) = info.anchor {
            let origin = anchor.world_position(carriers);
            commands.entity(entity).insert(grounding_diagnostics(
                collision_world,
                &origin,
                actor_physics,
                &plates.open_barrier_kinds,
                &[],
            ));
            planned_moves.push(CharacterMovePlan::from_target(
                entity,
                *pos,
                origin,
                0.0,
                actor_physics,
                false,
            ));
            continue;
        }
        let correction_displacement = match recon_option.as_mut() {
            Some(recon) => match reconcile_character(
                commands,
                entity,
                actor_id.0,
                &info.kind,
                &mut pos,
                &mut motion,
                recon,
                delta,
            ) {
                CharacterReconciliationOutcome::Displacement(displacement) => displacement,
                CharacterReconciliationOutcome::Snapped => {
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

        let step = step_actor_movement(ActorMovementStep {
            start: *pos,
            vertical_velocity: motion.0,
            intent: *move_intent,
            external_displacement: correction_displacement,
            delta,
            can_use_ladders: actor_config.can_use_ladders,
            physics: actor_physics,
            open_kinds: &plates.open_barrier_kinds,
            collision_world,
            map_settings,
            carriers,
        });
        commands.entity(entity).insert((step.grounding, step.support));
        push_actor_planned_move(
            planned_moves,
            actor_starts,
            CharacterMovePlan::from_movement_result(entity, *pos, step, actor_physics),
        );
    }
}

pub(crate) fn apply_actor_moves(
    query: &mut ActorMovementQuery,
    actors: &ActorMap,
    planned_moves: &[CharacterMovePlan],
) {
    for planned_move in planned_moves {
        let Ok((_, id, mut pos, _, mut motion, _)) = query.get_mut(planned_move.entity) else {
            continue;
        };

        if blocking_character_move_plan(planned_move, planned_moves).is_some()
            && actors.get(id).is_none_or(|actor| actor.anchor.is_none())
        {
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
