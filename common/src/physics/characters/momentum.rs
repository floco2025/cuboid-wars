use bevy_ecs::prelude::{Component, Query, Res};
use bevy_math::Vec3;
use bevy_time::Time;

use super::types::{CharacterMovementResult, CharacterSupport};
use crate::{math::PHYSICS_EPSILON, protocol::MapSettings};

// Component attached to character entities tracking persistent gravity-axis
// velocity. X/Z velocity is derived from intent each tick. Running on a ramp can
// add vertical displacement for that frame, but it is not stored as velocity.
#[derive(Component, Default)]
pub struct CharacterVerticalVelocity(pub f32);

// Horizontal blast shove, decaying linearly to zero. Movement planning (server
// and client prediction) reads `step` as extra displacement on top of the
// intent-derived target; the decay systems tick it down after movement so
// both sides integrate the same curve. The vertical part of a launch rides
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

// Horizontal velocity a body keeps while airborne: a portal exit's launch,
// or the velocity of a carrier it jumped or walked off. Constant in the
// air; movement planning clears it on landing or collision.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct AirborneMomentum(pub Vec3);

impl AirborneMomentum {
    #[must_use]
    pub fn step(&self, delta: f32) -> Vec3 {
        self.0 * delta
    }

    pub fn finish_step(&mut self, movement: &CharacterMovementResult) {
        if movement.support == CharacterSupport::Airborne && !movement.blocked {
            self.0 += movement.floor_velocity.with_y(0.0);
        } else {
            self.0 = Vec3::ZERO;
        }
    }
}

#[must_use]
pub fn momentum_displacement(
    knockback: Option<&KnockbackVelocity>,
    momentum: Option<&AirborneMomentum>,
    delta: f32,
) -> Vec3 {
    knockback.map_or(Vec3::ZERO, |velocity| velocity.step(delta))
        + momentum.map_or(Vec3::ZERO, |momentum| momentum.step(delta))
}

pub fn knockback_decay_system(
    time: Res<Time>,
    map_settings: Option<Res<MapSettings>>,
    mut knockbacks: Query<&mut KnockbackVelocity>,
) {
    let Some(map_settings) = map_settings else {
        return;
    };
    let delta = time.delta_secs();
    for mut knockback in &mut knockbacks {
        knockback.decay(delta, map_settings.movement.knockback.deceleration);
    }
}
