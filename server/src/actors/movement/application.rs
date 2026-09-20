use common::{
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::FieldId,
};

use super::{blocking_character_move_plan, query::ActorMovementQuery};
use crate::actors::ActorMap;

pub(crate) fn apply_actor_moves(
    query: &mut ActorMovementQuery,
    actors: &ActorMap,
    planned_moves: &[CharacterMovePlan],
    world: &CollisionWorld,
    open: &[FieldId],
) {
    for planned_move in planned_moves {
        let Ok((_, id, _, mut pos, mut motion, _, _, _, _, mut crushed, mut landing, character)) =
            query.get_mut(planned_move.entity)
        else {
            continue;
        };

        let overlapping_move = blocking_character_move_plan(planned_move, planned_moves);
        // Only flying actors plan moves that another body can reject.
        if overlapping_move.is_some() && actors.get(id).is_none_or(|actor| actor.anchor.is_none()) {
            motion.0 = 0.0;
            crushed.0 = planned_move.crushed && world.character_penetrates_solid(&pos, character.0.physics(), open);
            continue;
        }
        *pos = planned_move.target;
        motion.0 = planned_move.target_vertical_velocity;
        crushed.0 = planned_move.crushed;
        landing.0 = planned_move.impact_speed;
    }
}

#[cfg(test)]
#[path = "tests/application.rs"]
mod tests;
