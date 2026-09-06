use bevy::prelude::*;
use rand::{RngExt, rng};

use super::{
    firework::FireworkRocket,
    particles::{ParticleClouds, ParticleSpawn},
};
use crate::{config::ClientSettings, constants::MISSILE_BODY_LENGTH};
use common::protocol::MissileMarker;

// Hot-core orange, scaled by the configured emissive brightness (the cloud
// material is unlit). Fire particles are emitted from the tail nozzle in
// world space: the missile flies out from under them, drawing the trail.
const EXHAUST_BASE_COLOR: Vec3 = Vec3::new(1.0, 0.4, 0.088);

// Emits from the interpolated render transform (Update, not FixedUpdate) so
// the trail is continuous at any frame rate, for local and remote missiles
// alike.
pub fn missile_exhaust_system(
    time: Res<Time>,
    mut clouds: ResMut<ParticleClouds>,
    client_settings: Res<ClientSettings>,
    missiles: Query<&Transform, With<MissileMarker>>,
    rockets: Query<&Transform, With<FireworkRocket>>,
) {
    let config = client_settings.vfx.missile_exhaust;
    let expected = config.particles_per_sec * time.delta_secs();
    let base_color = EXHAUST_BASE_COLOR * config.emissive_brightness;
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
                rng.random_range(-config.jitter..config.jitter),
                rng.random_range(-config.jitter..config.jitter),
                rng.random_range(-config.jitter..config.jitter),
            );
            clouds.exhaust.spawn(ParticleSpawn {
                position: nozzle + jitter * 0.1,
                velocity: -flight_dir * config.back_speed * rng.random_range(0.6..1.4) + jitter,
                acceleration: Vec3::Y * config.rise_acceleration,
                start_size: config.particle_size * rng.random_range(0.6..1.3),
                end_size: 0.0,
                stretch: Vec3::ONE,
                fades: true,
                lifetime: config.particle_lifetime_secs * rng.random_range(0.7..1.3),
                color: base_color * rng.random_range(0.7..1.15),
            });
        }
    }
}
