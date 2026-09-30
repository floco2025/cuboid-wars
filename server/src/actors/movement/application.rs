use common::{
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::FieldId,
};

use super::{blocking_character_move_plan, query::FreeActorQuery};

// Applies the flying actors' plans. A plan another body swept into is
// rejected: the actor stays, loses its vertical velocity, and is crushed only
// where it already stands inside a carrier.
pub(crate) fn apply_flying_moves(
    query: &mut FreeActorQuery,
    blockers: &[CharacterMovePlan],
    moves: &[CharacterMovePlan],
    world: &CollisionWorld,
    open: &[FieldId],
) {
    for planned_move in moves {
        let Ok(mut actor) = query.get_mut(planned_move.entity) else {
            continue;
        };
        if blocking_character_move_plan(planned_move, blockers.iter().chain(moves)).is_some() {
            actor.vertical_velocity.0 = 0.0;
            actor.crushed.0 = planned_move.crushed
                && world.character_penetrates_solid(&actor.position, actor.character.0.physics(), open);
            continue;
        }
        *actor.position = planned_move.target;
        actor.vertical_velocity.0 = planned_move.target_vertical_velocity;
        actor.crushed.0 = planned_move.crushed;
        actor.landing.0 = planned_move.impact_speed;
    }
}

#[cfg(test)]
#[path = "tests/application.rs"]
mod tests;
