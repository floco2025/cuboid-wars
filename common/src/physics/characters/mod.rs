mod geometry;
mod movement;
mod movement_plan;
mod types;

pub use geometry::character_paths_intersect;
pub use movement::{position_has_floor_support, step_character_movement, try_start_player_jump};
pub use movement_plan::{
    blocking_character_move_plan, character_move_plan_is_blocked, character_move_plans_intersect,
    overlapping_character, overlaps_other_character,
};
pub use types::{CharacterMovePlan, CharacterMovementResult, CharacterVerticalVelocity};

#[cfg(test)]
mod tests;
