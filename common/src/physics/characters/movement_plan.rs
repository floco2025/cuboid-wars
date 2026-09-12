use super::{
    geometry::{character_axis_separation, character_paths_intersect, character_positions_intersect},
    types::CharacterMovePlan,
};
use crate::math::PHYSICS_EPSILON;
use bevy_math::Vec3;

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
    let separation = character_axis_separation(&candidate.start, candidate.physics, &other.start, other.physics);
    let end_separation = character_axis_separation(&candidate.target, candidate.physics, &other.target, other.physics);
    let relative_move = (Vec3::from(candidate.target) - Vec3::from(candidate.start))
        - (Vec3::from(other.target) - Vec3::from(other.start));
    target_distance_sq > start_distance_sq + PHYSICS_EPSILON * PHYSICS_EPSILON
        && relative_move.dot(separation) <= 0.0
        && end_separation.length_squared() + PHYSICS_EPSILON * PHYSICS_EPSILON >= separation.length_squared()
}
