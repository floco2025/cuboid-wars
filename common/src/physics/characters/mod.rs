mod ball_hits;
mod geometry;
mod movement;
mod movement_plan;
mod types;

pub use ball_hits::{BallCharacterHit, HitDirection, ball_character_hit, ball_overlaps_character};
pub use geometry::{character_center, character_overlaps_item, character_paths_intersect, character_shape};
pub use movement::{
    CharacterEnvironment, CharacterStep, position_has_floor_support, step_character_movement, try_start_player_jump,
};
pub use movement_plan::{blocking_character_move_plan, character_move_plan_is_blocked, overlapping_character};
pub use types::{CharacterMovePlan, CharacterMovementResult, CharacterVerticalVelocity, KnockbackVelocity};

#[cfg(test)]
mod tests;
