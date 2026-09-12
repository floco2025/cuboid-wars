mod geometry;
mod ladder;
mod momentum;
mod movement;
mod movement_plan;
mod player_control;
mod support;
mod types;

pub use geometry::{
    character_axis_separation, character_hitbox_center, character_hitbox_shape, character_movement_center,
    character_movement_pose, character_movement_shape, character_paths_intersect, character_positions_intersect,
};
pub use ladder::LadderMode;
pub use momentum::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity, knockback_decay_system};
pub(crate) use movement::character_controller;
pub use movement::{CharacterEnvironment, CharacterStep, player_jump_velocity, step_character_movement};
pub use movement_plan::character_move_plans_intersect;
pub use player_control::player_control_velocity;
pub use support::{grounding_diagnostics, position_has_floor_support};
pub use types::{CharacterMovePlan, CharacterMovementResult, CharacterSupport, GroundingDiagnostics};

#[cfg(test)]
mod tests;
