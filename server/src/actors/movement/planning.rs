use bevy::prelude::{Entity, Vec3};

use crate::actors::{ActorMap, ActorMode};
use common::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    physics::{CharacterMovePlan, CharacterSupport, CollisionWorld},
    protocol::{ActorMoveIntent, Position, SwitchState},
};

use super::{
    flight::{FlightMoveContext, select_flying_move},
    ordering::sorted_actor_plan_order,
    query::ActorMovementQuery,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_actor_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    switch_state: &SwitchState,
    carriers: &Carriers,
    actors: &ActorMap,
    actor_starts: &[(Entity, Position, CharacterPhysicsConfig)],
    query: &mut ActorMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let actor_order = sorted_actor_plan_order(query, actors);

    for actor_order in actor_order {
        let Ok((
            entity,
            id,
            actor_movement,
            pos,
            _,
            mut move_intent,
            mut face_yaw,
            mut support,
            knockback,
            _,
            _,
            character,
        )) = query.get_mut(actor_order.entity)
        else {
            continue;
        };
        let Some(info) = actors.get(id) else {
            continue;
        };
        let actor_physics = character.0.physics();
        let current_pos = *pos;
        if let Some(anchor) = info.anchor {
            *move_intent = ActorMoveIntent::Idle;
            *support = CharacterSupport::Ground;
            if let ActorMode::Engage { target_pos, .. } = info.mode {
                face_yaw.0 = direction_toward(&current_pos, &target_pos);
            }
            planned_moves.push(CharacterMovePlan::from_target(
                entity,
                current_pos,
                anchor.world_position(carriers),
                0.0,
                actor_physics,
                false,
            ));
            continue;
        }
        let actor_movement = actor_movement.expect("movable actor lacks movement settings");
        let move_context = FlightMoveContext {
            entity,
            pos: &current_pos,
            actor_physics,
            delta,
            collision_world,
            planned_moves,
            actor_starts,
            open_fields: &switch_state.open_fields,
            knockback_step: knockback.map_or(Vec3::ZERO, |velocity| velocity.step(delta)),
            carriers,
        };

        assert!(character.0.flies(), "ground actor missing surface navigation state");
        let selected = select_flying_move(
            &move_context,
            info,
            actor_movement.roam_speed,
            actor_movement.active_speed,
        );
        *move_intent = selected.intent;
        if let Some(direction) = selected.intent.direction() {
            face_yaw.0 = direction;
        } else if let ActorMode::Engage { target_pos, .. } = info.mode {
            face_yaw.0 = direction_toward(&current_pos, &target_pos);
        }
        *support = selected.step.support;
        planned_moves.push(CharacterMovePlan::from_movement_result(
            entity,
            current_pos,
            selected.step,
            actor_physics,
        ));
    }
}

fn direction_toward(pos: &Position, target: &Position) -> f32 {
    (target.x - pos.x).atan2(target.z - pos.z)
}
