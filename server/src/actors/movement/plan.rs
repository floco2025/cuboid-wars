use bevy::prelude::{Entity, Vec3};
use common::{
    config::CharacterPhysicsConfig,
    physics::{CharacterMovePlan, character_axis_separation, character_move_plans_intersect},
    protocol::Position,
};

#[must_use]
pub(crate) fn blocking_character_move_plan<'a>(
    candidate: &CharacterMovePlan,
    planned_moves: &'a [CharacterMovePlan],
) -> Option<&'a CharacterMovePlan> {
    planned_moves
        .iter()
        .find(|other| character_move_plan_blocks(candidate, other))
}

#[must_use]
pub(crate) fn character_move_plan_is_blocked(
    candidate: &CharacterMovePlan,
    planned_moves: &[CharacterMovePlan],
    character_starts: &[(Entity, Position, CharacterPhysicsConfig)],
) -> bool {
    if blocking_character_move_plan(candidate, planned_moves).is_some() {
        return true;
    }

    // `planned_moves` contains characters already processed this frame.
    // `character_starts` covers characters not planned yet, so treat only
    // those as stationary blockers.
    character_starts.iter().any(|(entity, pos, physics)| {
        if *entity == candidate.entity {
            return false;
        }
        if planned_moves.iter().any(|planned_move| planned_move.entity == *entity) {
            return false;
        }
        if position_is_behind_move_plan(candidate, pos, *physics) {
            return false;
        }
        let stationary_character = CharacterMovePlan::stationary(*entity, *pos, 0.0, *physics);
        character_move_plans_intersect(candidate, &stationary_character)
    })
}

fn character_move_plan_blocks(candidate: &CharacterMovePlan, other: &CharacterMovePlan) -> bool {
    other.entity != candidate.entity && character_move_plans_intersect(candidate, other)
}

fn position_is_behind_move_plan(
    candidate: &CharacterMovePlan,
    other_pos: &Position,
    other_physics: CharacterPhysicsConfig,
) -> bool {
    let movement = Vec3::from(candidate.target) - Vec3::from(candidate.start);
    let separation = character_axis_separation(&candidate.start, candidate.physics, other_pos, other_physics);
    movement.dot(separation) < 0.0
}

#[cfg(test)]
#[path = "tests/plan.rs"]
mod tests;
