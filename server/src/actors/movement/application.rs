use common::physics::CharacterMovePlan;

use super::{blocking_character_move_plan, query::ActorMovementQuery};
use crate::actors::ActorMap;

pub(crate) fn apply_actor_moves(
    query: &mut ActorMovementQuery,
    actors: &ActorMap,
    planned_moves: &[CharacterMovePlan],
) {
    for planned_move in planned_moves {
        let Ok((_, id, _, mut pos, mut motion, _, _, _, _, mut crushed, _)) = query.get_mut(planned_move.entity) else {
            continue;
        };

        let overlapping_move = blocking_character_move_plan(planned_move, planned_moves);
        if overlapping_move.is_some() && actors.get(id).is_none_or(|actor| actor.anchor.is_none()) {
            if actors.get(id).is_some_and(|actor| actor.flight.is_none()) {
                pos.y = planned_move.target.y;
            }
        } else {
            *pos = planned_move.target;
        }
        motion.0 = planned_move.target_vertical_velocity;
        crushed.0 = planned_move.crushed;
    }
}
