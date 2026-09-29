mod geometry;
mod ladder;
mod momentum;
mod movement;
mod movement_plan;
mod player_control;
mod player_step;
mod support;
mod types;

pub use geometry::{
    character_axis_separation, character_hitbox_center, character_hitbox_shape, character_movement_center,
    character_movement_pose, character_movement_shape, character_paths_intersect, character_positions_intersect,
};
pub use ladder::LadderMode;
pub use momentum::{CharacterVerticalVelocity, HorizontalVelocity, KnockbackVelocity, knockback_decay_system};
pub use movement::{CharacterEnvironment, CharacterStep, PlayerJump, player_jump, step_character_movement};
pub use movement_plan::character_move_plans_intersect;
pub use player_control::{accelerate_player, player_control_velocity};
pub use player_step::{PlayerMovementStep, PlayerStepResult, step_player_movement, step_player_movement_blocked};
pub use support::{grounding_diagnostics, position_has_floor_support};
pub use types::{CharacterMovePlan, CharacterMovementResult, CharacterSupport, GroundingDiagnostics};

#[cfg(test)]
mod tests;
