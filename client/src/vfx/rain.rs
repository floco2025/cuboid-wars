use bevy::{audio::Volume, prelude::*};
use rand::{RngExt, rng, rngs::ThreadRng};

use super::{
    beam::take_emissions,
    particles::{ParticleCloud, ParticleClouds, ParticleSpawn},
};
use crate::{
    cameras::MainCameraMarker,
    config::{AssetSet, ClientSettings, WeatherConfig},
};
use common::physics::CollisionWorld;

// Smoothing time constant for the 4 Hz snapshot steps. Short — the server
// already shapes the ramp in / fade out; this only hides the stair-steps.
const SMOOTHING_TAU_SECS: f32 = 0.5;
// Drops live long enough to fall this far below the camera, so elevated
// walkways still see rain streaking past. (Spawn height above the camera is
// the `weather.spawn_height` config.)
const FALL_DISTANCE: f32 = 14.0;
// Upward sky-exposure probe length: taller than any map.
const SKY_PROBE_DISTANCE: f32 = 60.0;
const MAX_DROPS_PER_FRAME: usize = 32;
// A falling drop reads as a thin vertical streak, not a cube: the particle's
// Y axis is stretched to this world length while `drop_size` stays the
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
// Slightly brighter than the streaks so impacts sparkle against wet ground.
const SPLASH_COLOR: Vec3 = Vec3::new(0.7, 0.75, 0.85);
const DROP_COLOR: Vec3 = Vec3::new(0.55, 0.6, 0.7);

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
// `weather.drops_per_second` is the only density knob.
pub fn rain_particles_system(
    time: Res<Time>,
    rain: Res<RainIntensity>,
    client_settings: Res<ClientSettings>,
    collision_world: Option<Res<CollisionWorld>>,
    mut clouds: ResMut<ParticleClouds>,
    camera: Query<&Transform, With<MainCameraMarker>>,
    mut credit: Local<f32>,
    mut pending_splashes: Local<Vec<PendingSplash>>,
) {
    let now = time.elapsed_secs();
    let mut rng = rng();
    let weather = &client_settings.weather;

    if rain.current >= RAIN_EPSILON
        && let Ok(camera) = camera.single()
    {
        let count = take_emissions(
            &mut credit,
            weather.drops_per_second * rain.current,
            time.delta_secs(),
            MAX_DROPS_PER_FRAME,
        );
        emit_drops(
            &mut clouds.drops,
            &mut rng,
            weather,
            collision_world.as_deref(),
            camera,
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
            spawn_splash(&mut clouds.splashes, &mut rng, splash.position, weather);
        } else {
            index += 1;
        }
    }
}

#[expect(clippy::too_many_arguments, reason = "emission threads the whole rain context")]
fn emit_drops(
    drops: &mut ParticleCloud,
    rng: &mut ThreadRng,
    weather: &WeatherConfig,
    collision_world: Option<&CollisionWorld>,
    camera: &Transform,
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
    let lead =
        Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero() * (weather.spawn_radius * weather.spawn_lead_fraction);
    let disc_center = camera.translation + lead;

    for _ in 0..count {
        // sqrt for uniform density over the disc area.
        let radius = weather.spawn_radius * rng.random_range(0.0_f32..1.0).sqrt();
        let angle = rng.random_range(0.0..std::f32::consts::TAU);
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
        let spawn_position = probe_origin + Vec3::Y * weather.spawn_height;
        // The landing surface decides the drop's exact flight time; the
        // splash is scheduled for that moment at that point. Drops fall
        // straight down so the raycast column IS the impact column. No
        // surface below = the drop falls into the void, no splash.
        let landing = collision_world
            .and_then(|world| world.ground_surface_below(spawn_position, weather.spawn_height + FALL_DISTANCE));
        let lifetime = match &landing {
            Some(hit) => (spawn_position.y - hit.point.y) / weather.fall_speed,
            None => (weather.spawn_height + FALL_DISTANCE) / weather.fall_speed,
        };
        if let Some(hit) = landing {
            pending_splashes.push(PendingSplash {
                position: hit.point + Vec3::Y * 0.02,
                due_secs: now + lifetime,
            });
        }
        drops.spawn(ParticleSpawn {
            position: spawn_position,
            velocity: Vec3::new(0.0, -weather.fall_speed, 0.0),
            acceleration: Vec3::ZERO,
            start_size: weather.drop_size,
            end_size: weather.drop_size,
            stretch: Vec3::new(1.0, STREAK_LENGTH / weather.drop_size, 1.0),
            fades: false,
            lifetime,
            color: DROP_COLOR,
        });
    }
}

// A few tiny droplets bouncing up under gravity at the impact point.
// Velocities and airtime derive from the configured bounce height and
// scatter radius: `v = √(2gh)` peaks at `splash_height`, the airtime is the
// full up-and-down arc, and the horizontal speed covers `splash_radius`
// within it. Each droplet dies as it lands.
fn spawn_splash(splashes: &mut ParticleCloud, rng: &mut ThreadRng, position: Vec3, weather: &WeatherConfig) {
    let peak_velocity = (2.0 * SPLASH_DROPLET_GRAVITY * weather.splash_height).sqrt();
    let airtime = 2.0 * peak_velocity / SPLASH_DROPLET_GRAVITY;
    let horizontal_speed = weather.splash_radius / airtime;
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
            start_size: weather.splash_size,
            end_size: weather.splash_size,
            stretch: Vec3::ONE,
            fades: true,
            lifetime: 2.0 * vertical / SPLASH_DROPLET_GRAVITY,
            color: SPLASH_COLOR,
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
    asset_set: Option<Res<AssetSet>>,
    mut loop_entity: Local<Option<Entity>>,
    mut sinks: Query<&mut AudioSink>,
) {
    let raining = rain.current >= RAIN_EPSILON;
    match *loop_entity {
        None if raining => {
            let Some(asset_set) = asset_set else { return };
            let entity = commands
                .spawn((
                    AudioPlayer::new(asset_server.load(asset_set.player_sound("rain").to_owned())),
                    PlaybackSettings::LOOP,
                ))
                .id();
            *loop_entity = Some(entity);
        }
        Some(entity) if !raining => {
            commands.entity(entity).despawn();
            *loop_entity = None;
        }
        Some(entity) => {
            if let Ok(mut sink) = sinks.get_mut(entity) {
                sink.set_volume(Volume::Linear(rain.current * client_settings.weather.rain_volume));
            }
        }
        None => {}
    }
}
