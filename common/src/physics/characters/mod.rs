mod ball_hits;
mod geometry;
mod knockback;
mod ladder;
mod movement;
mod movement_plan;
mod player_control;
mod player_movement;
mod support;
mod types;

pub use ball_hits::{BallCharacterHit, HitDirection, ball_character_hit, ball_overlaps_character};
pub use geometry::{
    character_center, character_overlaps_item, character_paths_intersect, character_shape,
    character_vertical_ranges_overlap,
};
pub use knockback::knockback_decay_system;
pub use movement::{CharacterEnvironment, CharacterStep, player_jump_velocity, step_character_movement};
pub use movement_plan::{blocking_character_move_plan, character_move_plan_is_blocked, overlapping_character};
pub use player_control::player_control_velocity;
pub use player_movement::{PlayerMovementStep, step_player_movement};
pub use support::position_has_floor_support;
pub use types::{
    AirborneMomentum, CharacterMovePlan, CharacterMovementResult, CharacterSupport, CharacterVerticalVelocity,
    KnockbackVelocity, momentum_displacement,
};

#[cfg(test)]
mod tests;
