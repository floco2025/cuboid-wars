use super::{
    geometry::{character_paths_intersect, character_positions_intersect},
    types::CharacterMovePlan,
};
use crate::math::PHYSICS_EPSILON;

// Whether two planned moves collide: bodies already touching count as
// colliding unless the move separates them.
#[must_use]
pub fn character_move_plans_intersect(candidate: &CharacterMovePlan, other: &CharacterMovePlan) -> bool {
    if character_positions_intersect(&candidate.start, candidate.physics, &other.start, other.physics) {
        return !character_move_plans_separate(candidate, other);
    }

    character_paths_intersect(
        &candidate.start,
        &candidate.target,
        candidate.physics,
        &other.start,
        &other.target,
        other.physics,
    )
}

fn character_move_plans_separate(candidate: &CharacterMovePlan, other: &CharacterMovePlan) -> bool {
    let start_distance_sq = candidate.start.distance_sq(&other.start);
    let target_distance_sq = candidate.target.distance_sq(&other.target);
    target_distance_sq > start_distance_sq + PHYSICS_EPSILON * PHYSICS_EPSILON
}
