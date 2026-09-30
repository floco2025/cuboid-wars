mod falling;
mod geometry;
mod ladder;
mod momentum;
mod movement;
mod movement_plan;
mod player_control;
mod player_flight;
mod support;
mod types;

pub use falling::{fall_distance_for_speed, landing_damage};
pub use geometry::{
    character_axis_separation, character_hitbox_center, character_hitbox_shape, character_movement_center,
    character_movement_pose, character_movement_shape, character_paths_intersect, character_positions_intersect,
};
pub use ladder::LadderMode;
pub use momentum::{CharacterVerticalVelocity, KnockbackVelocity, knockback_decay_system};
pub use movement::{
    CharacterEnvironment, CharacterStart, CharacterStep, character_passive_motion, step_character_movement,
    step_character_movement_from,
};
pub use movement_plan::character_move_plans_intersect;
pub use player_control::{
    PlayerWish, accelerate_player, player_control_velocity, player_move_speed, player_wish_velocity,
};
pub use player_flight::{
    PlayerFlightPortals, PlayerFlightState, PlayerFlightTick, flight_funnel_prediction, step_player_flight,
};
pub use support::{grounding_diagnostics, position_has_floor_support};
pub use types::{CharacterMovePlan, CharacterMovementResult, CharacterSupport, GroundingDiagnostics};

#[cfg(test)]
mod tests;
