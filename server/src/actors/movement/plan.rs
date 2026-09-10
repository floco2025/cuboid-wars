use bevy::prelude::Entity;
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
    let candidate_move_x = candidate.target.x - candidate.start.x;
    let candidate_move_z = candidate.target.z - candidate.start.z;
    let other_move_x = other.target.x - other.start.x;
    let other_move_z = other.target.z - other.start.z;

    let candidate_move_len_sq = candidate_move_x.mul_add(candidate_move_x, candidate_move_z * candidate_move_z);
    let other_move_len_sq = other_move_x.mul_add(other_move_x, other_move_z * other_move_z);
    if candidate_move_len_sq <= PHYSICS_EPSILON * PHYSICS_EPSILON
        || other_move_len_sq <= PHYSICS_EPSILON * PHYSICS_EPSILON
    {
        return false;
    }

    let to_other_start_x = other.start.x - candidate.start.x;
    let to_other_start_z = other.start.z - candidate.start.z;
    let other_starts_in_front = candidate_move_x.mul_add(to_other_start_x, candidate_move_z * to_other_start_z) > 0.0;
    let moving_same_way = candidate_move_x.mul_add(other_move_x, candidate_move_z * other_move_z) > 0.0;
    let final_positions_overlap =
        character_positions_intersect(&candidate.target, candidate.physics, &other.target, other.physics);

    other_starts_in_front && moving_same_way && !final_positions_overlap
}

fn position_is_behind_move_plan(candidate: &CharacterMovePlan, other_pos: &Position) -> bool {
    let move_x = candidate.target.x - candidate.start.x;
    let move_z = candidate.target.z - candidate.start.z;
    let move_len_sq = move_x.mul_add(move_x, move_z * move_z);
    if move_len_sq <= PHYSICS_EPSILON * PHYSICS_EPSILON {
        return false;
    }

    let other_x = other_pos.x - candidate.start.x;
    let other_z = other_pos.z - candidate.start.z;
    move_x.mul_add(other_x, move_z * other_z) < 0.0
}

#[cfg(test)]
#[path = "tests/plan.rs"]
mod tests;
