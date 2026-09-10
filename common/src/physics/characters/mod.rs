mod actor_movement;
mod ball_hits;
mod geometry;
mod ladder;
mod momentum;
mod movement;
mod movement_plan;
mod player_control;
mod player_movement;
mod player_state;
mod support;
mod types;

pub use actor_movement::{ActorMovementStep, step_actor_movement};
pub use ball_hits::{BallCharacterHit, HitDirection, ball_character_hit, ball_overlaps_character};
pub use geometry::{
    character_hitbox_center, character_movement_center, character_movement_pose, character_movement_shape,
    character_overlaps_item, character_paths_intersect, character_surface_distance,
};
pub use ladder::LadderMode;
pub use momentum::{
    AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity, knockback_decay_system, momentum_displacement,
};
pub use movement::{CharacterEnvironment, CharacterStep, player_jump_velocity, step_character_movement};
pub use movement_plan::{blocking_character_move_plan, character_move_plan_is_blocked, overlapping_character};
pub use player_control::player_control_velocity;
pub use player_movement::{PlayerMovementStep, step_player_movement};
pub use player_state::{PlayerMotionBundle, player_movement_state};
pub use support::{grounding_diagnostics, position_has_floor_support};
pub use types::{CharacterMovePlan, CharacterMovementResult, CharacterSupport, GroundingDiagnostics};

#[cfg(test)]
mod tests;
