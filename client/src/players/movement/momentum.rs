use bevy::prelude::*;
use common::physics::{CharacterMovementResult, CharacterSupport};

// Persistent player horizontal velocity, including ordinary locomotion,
// portal launches and inherited carrier motion. Ground control and collision
// projection are applied by the player step, never by input sampling.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct HorizontalVelocity(pub Vec3);

impl HorizontalVelocity {
    // A departure takes the ride along, a wall contact removes the velocity
    // into it, and a ladder holds.
    pub fn finish_step(&mut self, movement: &CharacterMovementResult) {
        if movement.support == CharacterSupport::Airborne {
            self.0 += movement.floor_velocity.with_y(0.0);
        }
        if movement.blocked {
            for normal in movement.contact_normals {
                if normal.y.abs() > 0.5 {
                    continue;
                }
                let n = normal.with_y(0.0).normalize_or_zero();
                self.0 -= n * self.0.dot(n).min(0.0);
            }
        }
        if movement.support == CharacterSupport::Ladder {
            self.0 = Vec3::ZERO;
        }
    }
}

#[cfg(test)]
#[path = "tests/momentum.rs"]
mod tests;
