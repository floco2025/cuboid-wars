use bevy_ecs::{
    prelude::{Component, Query, Res},
    query::QueryFilter,
};
use bevy_math::Vec3;
use bevy_time::Time;

use crate::{math::PHYSICS_EPSILON, protocol::MapSettings};

// Component attached to character entities tracking persistent gravity-axis
// velocity. Player X/Z velocity persists separately. Running on a ramp can
// add vertical displacement for that frame, but it is not stored as velocity.
#[derive(Component, Default)]
pub struct CharacterVerticalVelocity(pub f32);

// Horizontal blast shove, decaying linearly to zero. Movement planning reads
// `step` as extra displacement on top of the intent-derived target; the decay
// system ticks it down after movement. The vertical part of a launch rides
// `CharacterVerticalVelocity` instead.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct KnockbackVelocity(pub Vec3);

impl KnockbackVelocity {
    #[must_use]
    pub fn step(&self, delta: f32) -> Vec3 {
        self.0 * delta
    }

    pub fn decay(&mut self, delta: f32, deceleration: f32) {
        let speed = self.0.length();
        if speed <= PHYSICS_EPSILON {
            self.0 = Vec3::ZERO;
            return;
        }
        // Linear friction-style deceleration: hits zero exactly instead of
        // trailing off into an exponential crawl.
        let remaining = (speed - deceleration * delta).max(0.0);
        self.0 *= remaining / speed;
    }
}

// Persistent player horizontal velocity, including ordinary locomotion,
// portal launches and inherited carrier motion. Ground control and collision
// projection are applied by the shared player step, never by input sampling.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct HorizontalVelocity(pub Vec3);

impl HorizontalVelocity {
    pub fn finish_step(&mut self, movement: &super::CharacterMovementResult) {
        if movement.support == super::CharacterSupport::Airborne {
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
        if movement.support == super::CharacterSupport::Ladder {
            self.0 = Vec3::ZERO;
        }
    }
}

pub fn knockback_decay_system<F: QueryFilter>(
    time: Res<Time>,
    map_settings: Option<Res<MapSettings>>,
    mut knockbacks: Query<&mut KnockbackVelocity, F>,
) {
    let Some(map_settings) = map_settings else {
        return;
    };
    let delta = time.delta_secs();
    for mut knockback in &mut knockbacks {
        knockback.decay(delta, map_settings.movement.knockback.deceleration);
    }
}

#[cfg(test)]
#[path = "tests/momentum.rs"]
mod tests;
