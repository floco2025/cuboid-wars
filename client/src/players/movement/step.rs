use bevy::math::Vec3;
use common::physics::KnockbackVelocity;
pub use common::physics::{PlayerMovementStep, step_player_movement, step_player_movement_blocked};

#[must_use]
pub fn momentum_displacement(knockback: Option<&KnockbackVelocity>, delta: f32) -> Vec3 {
    knockback.map_or(Vec3::ZERO, |velocity| velocity.step(delta))
}
