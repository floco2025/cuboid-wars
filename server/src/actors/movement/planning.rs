use bevy::prelude::{Entity, Vec3};

use crate::actors::{ActorMap, ActorMode};
use common::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::{Position, SwitchState},
};

use super::{
    flight::{FlightMoveContext, select_flying_move},
    ordering::sorted_actor_plan_order,
    query::FreeActorQuery,
};

// Plans every flying actor's move, shortest remaining route first, each
// sweeping against `planned_moves` so far; the plans are appended to it.
// Anchored actors are placed by `anchored_actors_placement_system` and only
// block here.
pub(crate) fn plan_flying_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    switch_state: &SwitchState,
    carriers: &Carriers,
    actors: &ActorMap,
    actor_starts: &[(Entity, Position, CharacterPhysicsConfig)],
    query: &mut FreeActorQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    for order in sorted_actor_plan_order(query, actors) {
        let Ok(mut actor) = query.get_mut(order.entity) else {
            continue;
        };
        let Some(info) = actors.get(actor.id).filter(|info| info.anchor.is_none()) else {
            continue;
        };
        let actor_movement = actor.movement.expect("movable actor lacks movement settings");
        let actor_physics = actor.character.0.physics();
        let current_pos = *actor.position;
        let move_context = FlightMoveContext {
            entity: actor.entity,
            pos: &current_pos,
            actor_physics,
            delta,
            collision_world,
            planned_moves,
            actor_starts,
            open_fields: &switch_state.open_fields,
            knockback_step: actor.knockback.map_or(Vec3::ZERO, |velocity| velocity.step(delta)),
            carriers,
        };

        assert!(
            actor.character.0.flies(),
            "ground actor missing surface navigation state"
        );
        let selected = select_flying_move(
            &move_context,
            info,
            actor_movement.roam_speed,
            actor_movement.active_speed,
        );
        *actor.intent = selected.intent;
        if let Some(direction) = selected.intent.direction() {
            actor.face_yaw.0 = direction;
        } else if let ActorMode::Engage { target_pos, .. } = info.mode {
            actor.face_yaw.0 = (target_pos.x - current_pos.x).atan2(target_pos.z - current_pos.z);
        }
        *actor.support = selected.step.support;
        planned_moves.push(CharacterMovePlan::from_movement_result(
            actor.entity,
            current_pos,
            selected.step,
            actor_physics,
        ));
    }
}
