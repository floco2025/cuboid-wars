use bevy::prelude::*;
use rand::{RngExt, rng};

use super::{
    firework::FireworkRocket,
    particles::{ParticleClouds, ParticleSpawn},
};
use crate::constants::{
    MISSILE_BODY_LENGTH, MISSILE_EXHAUST_BACK_SPEED, MISSILE_EXHAUST_BASE_COLOR, MISSILE_EXHAUST_EMISSIVE_BRIGHTNESS,
    MISSILE_EXHAUST_JITTER, MISSILE_EXHAUST_PARTICLE_LIFETIME_SECS, MISSILE_EXHAUST_PARTICLE_SIZE,
    MISSILE_EXHAUST_PARTICLES_PER_SEC, MISSILE_EXHAUST_RISE_ACCELERATION,
};
use common::protocol::MissileMarker;

// Emits from the interpolated render transform (Update, not FixedUpdate) so
// the trail is continuous at any frame rate, for local and remote missiles
// alike.
pub fn missile_exhaust_system(
    time: Res<Time>,
    mut clouds: ResMut<ParticleClouds>,
    missiles: Query<&Transform, With<MissileMarker>>,
    rockets: Query<&Transform, With<FireworkRocket>>,
) {
    let expected = MISSILE_EXHAUST_PARTICLES_PER_SEC * time.delta_secs();
    let base_color = MISSILE_EXHAUST_BASE_COLOR * MISSILE_EXHAUST_EMISSIVE_BRIGHTNESS;
    let mut rng = rng();

    for transform in missiles.iter().chain(rockets.iter()) {
        let mut count = expected.floor() as usize;
        if rng.random_range(0.0..1.0) < expected.fract() {
            count += 1;
        }
        if count == 0 {
            continue;
        }

        // Meshes are Y-up: rotation * +Y is the nose, so the nozzle sits a
        // half body-length behind the origin.
        let flight_dir = transform.rotation * Vec3::Y;
        let nozzle = transform.translation - flight_dir * (MISSILE_BODY_LENGTH / 2.0 + 0.05);

        for _ in 0..count {
            let jitter = Vec3::new(
                rng.random_range(-MISSILE_EXHAUST_JITTER..MISSILE_EXHAUST_JITTER),
                rng.random_range(-MISSILE_EXHAUST_JITTER..MISSILE_EXHAUST_JITTER),
                rng.random_range(-MISSILE_EXHAUST_JITTER..MISSILE_EXHAUST_JITTER),
            );
            clouds.exhaust.spawn(ParticleSpawn {
                position: nozzle + jitter * 0.1,
                velocity: -flight_dir * MISSILE_EXHAUST_BACK_SPEED * rng.random_range(0.6..1.4) + jitter,
                acceleration: Vec3::Y * MISSILE_EXHAUST_RISE_ACCELERATION,
                start_size: MISSILE_EXHAUST_PARTICLE_SIZE * rng.random_range(0.6..1.3),
                end_size: 0.0,
                stretch: Vec3::ONE,
                fades: true,
                lifetime: MISSILE_EXHAUST_PARTICLE_LIFETIME_SECS * rng.random_range(0.7..1.3),
                color: base_color * rng.random_range(0.7..1.15),
            });
        }
    }
}
