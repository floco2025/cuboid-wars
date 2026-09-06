use std::f32::consts::FRAC_PI_2;

use bevy::prelude::Vec3;

use crate::{actors::ActorMap, map::PlateState, network::broadcast_to_all, players::PlayerMap};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    map::Carriers,
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::{ActorId, ActorMoveIntent, ActorMovementState, MapSettings, Position, SActorMove, ServerMessage},
};

use super::{
    context::{ActorMoveContext, CandidateStep, SelectedActorMove},
    ordering::sorted_actor_plan_order,
    query::ActorMovementQuery,
    steering::{ActorDesire, desired_move},
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_actor_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    map_settings: &MapSettings,
    players: &PlayerMap,
    plates: &PlateState,
    carriers: &Carriers,
    actors: &ActorMap,
    actor_starts: &[(bevy::prelude::Entity, Position, CharacterPhysicsConfig)],
    query: &mut ActorMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let actor_order = sorted_actor_plan_order(query, actors);

    for actor_order in actor_order {
        let Ok((entity, id, actor_movement, pos, motion, mut move_intent, mut face_yaw, knockback)) =
            query.get_mut(actor_order.entity)
        else {
            continue;
        };
        let Some(info) = actors.get(id) else {
            continue;
        };
        let actor_physics = gameplay_config.expect_actor(&info.spawn_kind).physics();
        let current_pos = *pos;
        let move_context = ActorMoveContext {
            entity,
            pos: &current_pos,
            vertical_velocity: motion.0,
            actor_physics,
            delta,
            collision_world,
            planned_moves,
            actor_starts,
            open_barrier_kinds: &plates.open_barrier_kinds,
            gravity: map_settings.movement.gravity,
            ladder_climb_ratio: map_settings.movement.ladder_climb_ratio,
            knockback_step: knockback.map_or(Vec3::ZERO, |velocity| velocity.step(delta)),
            carriers,
        };

        let mut hold_facing = None;
        let selected = match desired_move(
            info,
            &current_pos,
            actor_movement.roam_speed,
            actor_movement.active_speed,
        ) {
            ActorDesire::Idle => move_context.idle_move(),
            ActorDesire::HoldFacing { direction } => {
                hold_facing = Some(direction);
                move_context.idle_move()
            }
            ActorDesire::Move { intent, target } => select_route_move(&move_context, intent, &target),
        };

        let changed = *move_intent != selected.intent;
        *move_intent = selected.intent;
        if let Some(direction) = selected.intent.direction().or(hold_facing) {
            face_yaw.0 = direction;
        }
        if changed {
            broadcast_actor_move_intent(players, *id, current_pos, selected.intent, motion.0);
        }
        planned_moves.push(CharacterMovePlan::from_movement_result(
            entity,
            current_pos,
            selected.step,
            actor_physics,
        ));
    }
}

pub(super) fn select_route_move(
    context: &ActorMoveContext<'_>,
    desired: ActorMoveIntent,
    target: &Position,
) -> SelectedActorMove {
    match context.evaluate(desired, target, false) {
        CandidateStep::Clear(selected) | CandidateStep::Graze(selected) => selected,
        CandidateStep::Blocked => context.idle_move(),
        CandidateStep::CharacterBlocked => {
            let direction = desired.direction().expect("route movement has a direction");
            let speed = desired.speed().expect("route movement has a speed");
            for sidestep_direction in [direction + FRAC_PI_2, direction - FRAC_PI_2] {
                let sidestep = ActorMoveIntent::Moving {
                    direction: sidestep_direction,
                    speed,
                };
                if let CandidateStep::Clear(selected) = context.evaluate(sidestep, target, true) {
                    return selected;
                }
            }
            context.idle_move()
        }
    }
}

fn broadcast_actor_move_intent(
    players: &PlayerMap,
    id: ActorId,
    pos: Position,
    move_intent: ActorMoveIntent,
    vertical_velocity: f32,
) {
    broadcast_to_all(
        players,
        ServerMessage::ActorMove(SActorMove {
            id,
            movement: ActorMovementState::new(pos, move_intent, vertical_velocity),
        }),
    );
}
