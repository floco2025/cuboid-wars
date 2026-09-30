mod feedback;
#[cfg(test)]
#[path = "tests/fixtures.rs"]
mod fixtures;
mod interpolation;
mod jump;
mod local;
mod momentum;
mod outcomes;
mod owner;
mod planning;
mod reports;
mod step;

pub(crate) use interpolation::interpolate_remote_players_system;
pub use jump::{PlayerJump, player_jump};
pub(crate) use local::{local_player_feedback_system, local_player_movement_system};
pub use momentum::HorizontalVelocity;
pub use outcomes::{LocalMovementStep, collect_move_outcomes};
pub use owner::{JumpRequest, OwnerBody, OwnerTickOutcome, OwnerWorld, ladder_facing, owner_tick};
pub use planning::{PlayerMove, plan_player_move};
pub use reports::LocalMovementReports;
pub use step::{PlayerMovementStep, PlayerStepResult, step_player_movement, step_player_movement_blocked};
