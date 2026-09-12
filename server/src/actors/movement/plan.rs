use bevy::prelude::{Entity, Vec3};
use common::{
    config::CharacterPhysicsConfig,
    math::PHYSICS_EPSILON,
    physics::{CharacterMovePlan, character_move_plans_intersect, character_positions_intersect},
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
        if position_is_behind_move_plan(candidate, pos) {
            return false;
        }
        let stationary_character = CharacterMovePlan::stationary(*entity, *pos, 0.0, *physics);
        character_move_plans_intersect(candidate, &stationary_character)
    })
}

fn character_move_plan_blocks(candidate: &CharacterMovePlan, other: &CharacterMovePlan) -> bool {
    if other.entity == candidate.entity || !character_move_plans_intersect(candidate, other) {
        return false;
    }

    !character_move_plan_follows_front_move(candidate, other)
}

fn character_move_plan_follows_front_move(candidate: &CharacterMovePlan, other: &CharacterMovePlan) -> bool {
    let candidate_move = Vec3::from(candidate.target) - Vec3::from(candidate.start);
    let other_move = Vec3::from(other.target) - Vec3::from(other.start);
    if candidate_move.length_squared() <= PHYSICS_EPSILON * PHYSICS_EPSILON
        || other_move.length_squared() <= PHYSICS_EPSILON * PHYSICS_EPSILON
    {
        return false;
    }
    let to_other = Vec3::from(other.start) - Vec3::from(candidate.start);
    candidate_move.dot(to_other) > 0.0
        && candidate_move.dot(other_move) > 0.0
        && !character_positions_intersect(&candidate.target, candidate.physics, &other.target, other.physics)
}

fn position_is_behind_move_plan(candidate: &CharacterMovePlan, other_pos: &Position) -> bool {
    let movement = Vec3::from(candidate.target) - Vec3::from(candidate.start);
    movement.dot(Vec3::from(*other_pos) - Vec3::from(candidate.start)) < 0.0
}

#[cfg(test)]
#[path = "tests/plan.rs"]
mod tests;
