use crate::constants::{
    RAIN_DROP_COLOR, RAIN_FALL_SPEED, RAIN_SPAWN_HEIGHT, RAIN_SPLASH_COLOR, RAIN_SPLASH_HEIGHT, RAIN_SPLASH_RADIUS,
    RAIN_SPLASH_SIZE,
};
use bevy::{
    audio::{GlobalVolume, Volume},
    prelude::*,
};
use rand::{RngExt, rng, rngs::ThreadRng};
use std::f32::consts::TAU;

use super::{
    beam::take_emissions,
    particles::{ParticleCloud, ParticleClouds, ParticleSpawn},
};
use crate::{
    audio::play_sound_with,
    cameras::MainCameraMarker,
    config::{AssetSet, ClientSettings, WeatherConfig},
};
use common::physics::CollisionWorld;

// Smoothing time constant for the 4 Hz snapshot steps. Short — the server
// already shapes the ramp in / fade out; this only hides the stair-steps.
const SMOOTHING_TAU_SECS: f32 = 0.5;
// Drops live long enough to fall this far below the camera, so elevated
// walkways still see rain streaking past. (Spawn height above the camera is
// the `RAIN_SPAWN_HEIGHT` config.)
const FALL_DISTANCE: f32 = 14.0;
// Upward sky-exposure probe length: taller than any map.
const SKY_PROBE_DISTANCE: f32 = 60.0;
const MAX_DROPS_PER_FRAME: usize = 32;
// A falling drop reads as a thin vertical streak, not a cube: the particle's
// Y axis is stretched to this world length while `rain_drop_size` stays the
// cross-section.
const STREAK_LENGTH: f32 = 0.28;
// No drops inside this horizontal radius of the camera — a streak half a
// meter from the lens renders as a giant bar across the screen.
const CAMERA_CLEARANCE_RADIUS: f32 = 1.5;
// Splash on impact: a few tiny droplets bouncing up under gravity, spawned
// the moment a drop's precomputed flight time expires at its raycast
// landing point. Droplets fade (dying sparkle); the streaks themselves
// don't — the pool is opaque, so a "faded" streak is a black bar.
const SPLASH_DROPLET_COUNT: usize = 3;
const SPLASH_DROPLET_GRAVITY: f32 = 12.0;

// The particle clouds take raw linear-RGB `Vec3` vertex colors.
fn linear_rgb(color: Color) -> Vec3 {
    let linear = color.to_linear();
    Vec3::new(linear.red, linear.green, linear.blue)
}

// A drop already in flight, due to splash at `due_secs` (elapsed time) where
// its landing raycast hit.
pub struct PendingSplash {
    position: Vec3,
    due_secs: f32,
}
// Below this the rain is inaudible/invisible and the loop entity is torn down.
const RAIN_EPSILON: f32 = 0.01;

// Authoritative rain intensity from the snapshot (`target`), smoothed
// locally (`current`) — every rain visual and the loop volume read `current`.
#[derive(Resource, Default)]
pub struct RainIntensity {
    pub target: f32,
    pub current: f32,
}

pub fn rain_smoothing_system(time: Res<Time>, mut rain: ResMut<RainIntensity>) {
    let blend = 1.0 - (-time.delta_secs() / SMOOTHING_TAU_SECS).exp();
    rain.current += (rain.target - rain.current) * blend;
}

// Emit falling drops in a disc around the camera, only in columns open to
// the sky (an upward probe from camera height finds any roof/floor above —
// no indoor rain). The particle clouds grow on demand, so
// `client.json::weather.rain_drops_per_second` is the only density knob.
pub fn rain_particles_system(
    time: Res<Time>,
    rain: Res<RainIntensity>,
    client_settings: Res<ClientSettings>,
    collision_world: Res<CollisionWorld>,
    mut clouds: ResMut<ParticleClouds>,
    camera: Query<&Transform, With<MainCameraMarker>>,
    mut credit: Local<f32>,
    mut pending_splashes: Local<Vec<PendingSplash>>,
) {
    let now = time.elapsed_secs();
    let mut rng = rng();

    if rain.current >= RAIN_EPSILON
        && let Ok(camera) = camera.single()
    {
        let weather = &client_settings.weather;
        let count = take_emissions(
            &mut credit,
            weather.rain_drops_per_second * rain.current,
            time.delta_secs(),
            MAX_DROPS_PER_FRAME,
        );
        emit_drops(
            &mut clouds.drops,
            &mut rng,
            Some(&collision_world),
            camera,
            weather,
            count,
            now,
            &mut pending_splashes,
        );
    }

    // Land drops already in flight — runs even while the rain is dying out,
    // so what's falling still hits the ground.
    let mut index = 0;
    while index < pending_splashes.len() {
        if pending_splashes[index].due_secs <= now {
            let splash = pending_splashes.swap_remove(index);
            spawn_splash(&mut clouds.splashes, &mut rng, splash.position);
        } else {
            index += 1;
        }
    }
}

fn emit_drops(
    drops: &mut ParticleCloud,
    rng: &mut ThreadRng,
    collision_world: Option<&CollisionWorld>,
    camera: &Transform,
    weather: &WeatherConfig,
    count: usize,
    now: f32,
    pending_splashes: &mut Vec<PendingSplash>,
) {
    // The spawn disc leads the camera along its horizontal facing so
    // sprinting forward doesn't outrun the volume (drops also need ~0.6 s
    // to fall to eye level, during which a runner covers several meters) —
    // at the default 1/3 lead, two thirds of the rain falls ahead. Top-down
    // view has no horizontal facing — the projection degenerates to zero
    // and the disc stays centered.
    let forward = camera.forward();
    let lead = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero()
        * (weather.rain_spawn_radius * weather.spawn_lead_fraction);
    let disc_center = camera.translation + lead;

    for _ in 0..count {
        // sqrt for uniform density over the disc area.
        let radius = weather.rain_spawn_radius * rng.random_range(0.0_f32..1.0).sqrt();
        let angle = rng.random_range(0.0..TAU);
        let x = disc_center.x + radius * angle.cos();
        let z = disc_center.z + radius * angle.sin();
        // Never right at the lens — a streak half a meter away renders as a
        // screen-wide bar. Checked against the camera, not the disc center.
        let to_camera_x = x - camera.translation.x;
        let to_camera_z = z - camera.translation.z;
        if to_camera_x * to_camera_x + to_camera_z * to_camera_z < CAMERA_CLEARANCE_RADIUS * CAMERA_CLEARANCE_RADIUS {
            continue;
        }
        let probe_origin = Vec3::new(x, camera.translation.y, z);
        let covered = collision_world.is_some_and(|world| {
            world
                .world_surface_along_ray(probe_origin, Vec3::Y, SKY_PROBE_DISTANCE)
                .is_some()
        });
        if covered {
            continue;
        }
        let spawn_position = probe_origin + Vec3::Y * RAIN_SPAWN_HEIGHT;
        // The landing surface decides the drop's exact flight time; the
        // splash is scheduled for that moment at that point. Drops fall
        // straight down so the raycast column IS the impact column. No
        // surface below = the drop falls into the void, no splash.
        let landing = collision_world
            .and_then(|world| world.ground_surface_below(spawn_position, RAIN_SPAWN_HEIGHT + FALL_DISTANCE));
        let lifetime = match &landing {
            Some(hit) => (spawn_position.y - hit.point.y) / RAIN_FALL_SPEED,
            None => (RAIN_SPAWN_HEIGHT + FALL_DISTANCE) / RAIN_FALL_SPEED,
        };
        if let Some(hit) = landing {
            pending_splashes.push(PendingSplash {
                position: hit.point + Vec3::Y * 0.02,
                due_secs: now + lifetime,
            });
        }
        drops.spawn(ParticleSpawn {
            position: spawn_position,
            velocity: Vec3::new(0.0, -RAIN_FALL_SPEED, 0.0),
            acceleration: Vec3::ZERO,
            start_size: weather.rain_drop_size,
            end_size: weather.rain_drop_size,
            stretch: Vec3::new(1.0, STREAK_LENGTH / weather.rain_drop_size, 1.0),
            fades: false,
            lifetime,
            color: linear_rgb(RAIN_DROP_COLOR),
        });
    }
}

// A few tiny droplets bouncing up under gravity at the impact point.
// Velocities and airtime derive from the configured bounce height and
// scatter radius: `v = √(2gh)` peaks at `splash_height`, the airtime is the
// full up-and-down arc, and the horizontal speed covers `splash_radius`
// within it. Each droplet dies as it lands.
fn spawn_splash(splashes: &mut ParticleCloud, rng: &mut ThreadRng, position: Vec3) {
    let peak_velocity = (2.0 * SPLASH_DROPLET_GRAVITY * RAIN_SPLASH_HEIGHT).sqrt();
    let airtime = 2.0 * peak_velocity / SPLASH_DROPLET_GRAVITY;
    let horizontal_speed = RAIN_SPLASH_RADIUS / airtime;
    for _ in 0..SPLASH_DROPLET_COUNT {
        let vertical = peak_velocity * rng.random_range(0.8..1.1);
        splashes.spawn(ParticleSpawn {
            position,
            velocity: Vec3::new(
                rng.random_range(-horizontal_speed..=horizontal_speed),
                vertical,
                rng.random_range(-horizontal_speed..=horizontal_speed),
            ),
            acceleration: Vec3::new(0.0, -SPLASH_DROPLET_GRAVITY, 0.0),
            start_size: RAIN_SPLASH_SIZE,
            end_size: RAIN_SPLASH_SIZE,
            stretch: Vec3::ONE,
            fades: true,
            lifetime: 2.0 * vertical / SPLASH_DROPLET_GRAVITY,
            color: linear_rgb(RAIN_SPLASH_COLOR),
        });
    }
}

// One global (non-spatial) looping rain sound while it rains; its volume
// tracks the smoothed intensity, so the server's fade envelope is the
// audio fade. Dropping the entity drops the sink and stops the loop.
pub fn rain_audio_system(
    mut commands: Commands,
    rain: Res<RainIntensity>,
    client_settings: Res<ClientSettings>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    mut loop_entity: Local<Option<Entity>>,
    global_volume: Res<GlobalVolume>,
    mut sinks: Query<&mut AudioSink>,
) {
    let raining = rain.current >= RAIN_EPSILON;
    match *loop_entity {
        None if raining => {
            let entity = play_sound_with(
                &mut commands,
                &asset_server,
                asset_set.player_sound("rain"),
                PlaybackSettings::LOOP,
            );
            *loop_entity = Some(entity);
        }
        Some(entity) if !raining => {
            commands.entity(entity).despawn();
            *loop_entity = None;
        }
        Some(entity) => {
            if let Ok(mut sink) = sinks.get_mut(entity) {
                // Runs after `apply_global_volume_system` (ordered before
                // `ClientSet::Sky`), so this per-frame write wins its push.
                sink.set_volume(
                    Volume::Linear(rain.current * client_settings.audio.rain_volume) * global_volume.volume,
                );
            }
        }
        None => {}
    }
}
