use std::{collections::VecDeque, f32::consts::TAU};

use bevy::{audio::SpatialScale, ecs::system::SystemParam, light::NotShadowCaster, prelude::*};
use rand::{RngExt, SeedableRng, rngs::StdRng};

use crate::{
    audio::{play_explosion_sound, play_spatial_sound, sound_playback},
    carriers::CarrierEntities,
    config::{AssetSet, ClientSettings},
    constants::LASER_EMISSIVE,
    map::MapDimensions,
    missiles::{MissileAssets, missile_rotation, spawn_missile_meshes},
    players::MyPlayerId,
    projectiles::{ProjectileAssets, spawn_ember_projectile},
    vfx::{BlastRadii, ExplosionAssets, ExplosionSpawnCtx, ExplosionVfxBudget, spawn_missile_explosion},
};
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{MapLayout, Position},
};

// ============================================================================
// Show shape
// ============================================================================
// The whole ~33 s choreography is derived up front from the broadcast seed,
// so every client plays an identical show. All randomness is resolved at
// build time; playback is deterministic. The show is sized by the map: each
// fraction below is of the map's size, the larger of its planar radius and
// its height, so a wide map spreads the show and a tall one lifts it.

// Every rocket flies this long whatever the map, keeping the show's rhythm;
// the fuse is the exact flight time, so the pop lands on the aimed point.
const ROCKET_FLIGHT_SECS: f32 = 4.5;
const STAR_SPEED: f32 = 10.0;
const STAR_FUSE_SECS: f32 = 0.55;
// Launch ring outside the footprint, starting below the ground floor.
const RING_MARGIN: f32 = 0.15;
const ORIGIN_DEPTH: f32 = 0.1;
// Pops happen above the tallest storey by the clearance plus up to the
// jitter, so blasts are pure sky decoration, spread over this fraction of
// the footprint.
const SKY_CLEARANCE: f32 = 0.35;
const SKY_JITTER: f32 = 0.25;
const SKY_SPREAD: f32 = 0.6;
const EMBERS_PER_POP: usize = 14;
// Sky lasers: beams long enough to read as infinite, pivoting on ring
// points and sweeping across the sky.
const BEAM_LENGTH: f32 = 1200.0;
const BEAM_RADIUS: f32 = 0.12;
const BEAM_FADE_SECS: f32 = 0.6;

enum FireworkAction {
    Launch { pos: Vec3, velocity: Vec3, fuse_secs: f32 },
    Embers { pos: Vec3, velocities: Vec<Vec3> },
    LaserBeams { beams: Vec<LaserBeamSpec> },
}

// A sweeping sky laser: an effectively infinite beam through `pivot`,
// starting along `start_dir` and rotating around `sweep_axis` at
// `sweep_rate` rad/s for `duration_secs`. Deterministic in elapsed time, so
// every client renders the identical sweep.
#[derive(Clone, Copy)]
struct LaserBeamSpec {
    pivot: Vec3,
    start_dir: Vec3,
    sweep_axis: Vec3,
    sweep_rate: f32,
    duration_secs: f32,
}

struct FireworkEvent {
    at_secs: f32,
    action: FireworkAction,
}

#[derive(Resource, Default)]
pub struct FireworkShow {
    elapsed: f32,
    events: VecDeque<FireworkEvent>,
}

impl FireworkShow {
    // The fireworks switch spaces its shows by `FIREWORK_SHOW_SECS` plus the
    // map's cooldown, so only `/firework` can arrive mid-show; the show that
    // is still playing wins over its seed.
    pub fn start(&mut self, seed: u64, map: MapDimensions) {
        if !self.events.is_empty() {
            return;
        }
        self.elapsed = 0.0;
        self.events = build_show(seed, map);
    }
}

// A cosmetic rocket in flight. Deliberately NOT a `MissileMarker` entity —
// the missile snapshot diff, transform sync, and interpolation must never see
// show props.
#[derive(Component)]
pub struct FireworkRocket {
    velocity: Vec3,
    fuse_secs: f32,
}

#[derive(Component)]
pub struct FireworkLaser {
    spec: LaserBeamSpec,
    age_secs: f32,
    material: Handle<StandardMaterial>,
}

// ============================================================================
// Choreography
// ============================================================================

// The map's footprint is centred on the world origin, so the field is too.
struct ShowField {
    half_x: f32,
    half_z: f32,
    ring_radius: f32,
    origin_y: f32,
    sky_base: f32,
    sky_jitter: f32,
}

impl ShowField {
    fn new(map: MapDimensions) -> Self {
        let half_x = map.width / 2.0;
        let half_z = map.depth / 2.0;
        let radius = half_x.hypot(half_z);
        let size = radius.max(map.height);
        Self {
            half_x,
            half_z,
            ring_radius: radius + RING_MARGIN * size,
            origin_y: -ORIGIN_DEPTH * size,
            sky_base: map.height + SKY_CLEARANCE * size,
            sky_jitter: SKY_JITTER * size,
        }
    }
}

fn build_show(seed: u64, map: MapDimensions) -> VecDeque<FireworkEvent> {
    let field = ShowField::new(map);
    let mut rng = StdRng::seed_from_u64(seed);
    let mut events: Vec<FireworkEvent> = Vec::new();

    // Act 1 — opening volley: lone rockets finding their range.
    for i in 0..6 {
        let t = i as f32 + rng.random_range(0.0..0.6);
        rocket(&mut events, &mut rng, &field, t, false);
    }
    // Act 2 — star shells: every pop rings into a sphere of second stages
    // and rains embers.
    for i in 0..5 {
        let t = 6.5 + 1.5 * i as f32 + rng.random_range(0.0..0.7);
        rocket(&mut events, &mut rng, &field, t, true);
    }
    // Act 3 — laser show: long sweeping beams crossing the sky, no rockets.
    events.push(FireworkEvent {
        at_secs: 14.5,
        action: FireworkAction::LaserBeams {
            beams: (0..8).map(|_| beam(&mut rng, &field, 7.0, 0.35..0.9)).collect(),
        },
    });
    // Act 4 — finale: fast laser sweeps over a rolling volley, closing with
    // an everything-at-once barrage.
    events.push(FireworkEvent {
        at_secs: 21.0,
        action: FireworkAction::LaserBeams {
            beams: (0..6).map(|_| beam(&mut rng, &field, 9.5, 1.2..2.2)).collect(),
        },
    });
    for i in 0..16 {
        let t = 21.5 + 0.4 * i as f32 + rng.random_range(0.0..0.25);
        rocket(&mut events, &mut rng, &field, t, i % 2 == 0);
    }
    for _ in 0..20 {
        let t = 28.0 + rng.random_range(0.0..1.8);
        rocket(&mut events, &mut rng, &field, t, true);
    }

    events.sort_by(|a, b| a.at_secs.total_cmp(&b.at_secs));
    events.into()
}

// One rocket: launch from a random ring point outside/below the map, aimed
// at a random sky point over the field, popping on the aimed point. An
// optional star burst is scheduled at that precomputed pop.
fn rocket(events: &mut Vec<FireworkEvent>, rng: &mut StdRng, field: &ShowField, at_secs: f32, stars: bool) {
    let ring_angle = rng.random_range(0.0..TAU);
    let origin = Vec3::new(
        ring_angle.cos() * field.ring_radius,
        field.origin_y,
        ring_angle.sin() * field.ring_radius,
    );
    let sky = Vec3::new(
        rng.random_range(-SKY_SPREAD..SKY_SPREAD) * field.half_x,
        field.sky_base + rng.random_range(0.0..field.sky_jitter),
        rng.random_range(-SKY_SPREAD..SKY_SPREAD) * field.half_z,
    );
    events.push(FireworkEvent {
        at_secs,
        action: FireworkAction::Launch {
            pos: origin,
            velocity: (sky - origin) / ROCKET_FLIGHT_SECS,
            fuse_secs: ROCKET_FLIGHT_SECS,
        },
    });

    let pop_at = at_secs + ROCKET_FLIGHT_SECS;
    if stars {
        for _ in 0..7 {
            let dir = Sphere::new(1.0).sample_boundary(rng);
            events.push(FireworkEvent {
                at_secs: pop_at,
                action: FireworkAction::Launch {
                    pos: sky,
                    velocity: dir * STAR_SPEED,
                    fuse_secs: STAR_FUSE_SECS,
                },
            });
        }
    }
    // Every pop rains embers: glowing presentation projectiles that arc down
    // and bounce off rooftops. Velocities are precomputed so all clients
    // agree.
    let velocities = (0..EMBERS_PER_POP)
        .map(|_| {
            let side = rng.random_range(0.0..TAU);
            let speed = rng.random_range(3.0..8.0);
            Vec3::new(side.cos() * speed, rng.random_range(-2.0..4.0), side.sin() * speed)
        })
        .collect();
    events.push(FireworkEvent {
        at_secs: pop_at,
        action: FireworkAction::Embers { pos: sky, velocities },
    });
}

// One sweeping sky beam: pivot on the launch ring at ground level, tilted
// well above the horizon, rotating around vertical like a searchlight.
fn beam(rng: &mut StdRng, field: &ShowField, duration_secs: f32, sweep_rate: std::ops::Range<f32>) -> LaserBeamSpec {
    let ring_angle = rng.random_range(0.0..TAU);
    let pivot = Vec3::new(
        ring_angle.cos() * field.ring_radius,
        0.0,
        ring_angle.sin() * field.ring_radius,
    );
    // Tilt from vertical: 25°..65° — always aimed over the field, never flat
    // into buildings.
    let tilt: f32 = rng.random_range(0.44..1.13);
    let toward = -pivot.normalize_or_zero();
    let start_dir = (Vec3::Y * tilt.cos() + toward * tilt.sin()).normalize();
    let rate = rng.random_range(sweep_rate);
    LaserBeamSpec {
        pivot,
        start_dir,
        sweep_axis: Vec3::Y,
        // Alternate sweep directions.
        sweep_rate: if rng.random_range(0.0..1.0) < 0.5 { rate } else { -rate },
        duration_secs,
    }
}

// ============================================================================
// Playback
// ============================================================================

#[derive(SystemParam)]
pub struct FireworkVfx<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    budget: ResMut<'w, ExplosionVfxBudget>,
    explosion_assets: Res<'w, ExplosionAssets>,
    blast_radii: Res<'w, BlastRadii>,
    gameplay_config: Res<'w, GameplayConfig>,
    collision_world: Res<'w, CollisionWorld>,
    map_layout: Res<'w, MapLayout>,
    carriers: Res<'w, Carriers>,
    carrier_entities: Res<'w, CarrierEntities>,
}

#[derive(SystemParam)]
pub struct FireworkAssets<'w> {
    asset_server: Res<'w, AssetServer>,
    asset_set: Res<'w, AssetSet>,
    client_settings: Res<'w, ClientSettings>,
    missile_assets: Res<'w, MissileAssets>,
    projectile_assets: Res<'w, ProjectileAssets>,
    my_player_id: Res<'w, MyPlayerId>,
}

pub fn firework_system(
    mut commands: Commands,
    time: Res<Time>,
    mut show: ResMut<FireworkShow>,
    mut rockets: Query<(Entity, &mut FireworkRocket, &mut Transform), Without<FireworkLaser>>,
    mut lasers: Query<(Entity, &mut FireworkLaser, &mut Transform), Without<FireworkRocket>>,
    mut vfx: FireworkVfx,
    assets: FireworkAssets,
) {
    let delta = time.delta_secs();

    // Fly the rockets; a burnt fuse pops the shell where it is (which is the
    // precomputed aim point — straight flight is deterministic).
    for (entity, mut rocket, mut transform) in &mut rockets {
        transform.translation += rocket.velocity * delta;
        rocket.fuse_secs -= delta;
        if rocket.fuse_secs <= 0.0 {
            pop(&mut commands, &mut vfx, &assets, transform.translation);
            commands.entity(entity).despawn();
        }
    }

    // Sweep, fade, and expire the sky lasers. Orientation is a pure
    // function of age, so all clients render the same sweep.
    for (entity, mut laser, mut transform) in &mut lasers {
        laser.age_secs += delta;
        let spec = laser.spec;
        if laser.age_secs >= spec.duration_secs {
            commands.entity(entity).despawn();
            continue;
        }
        let dir = Quat::from_axis_angle(spec.sweep_axis, spec.sweep_rate * laser.age_secs) * spec.start_dir;
        transform.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
        if let Some(mut material) = vfx.materials.get_mut(&laser.material) {
            let fade_in = (laser.age_secs / BEAM_FADE_SECS).clamp(0.0, 1.0);
            let fade_out = ((spec.duration_secs - laser.age_secs) / BEAM_FADE_SECS).clamp(0.0, 1.0);
            let glow = LASER_EMISSIVE * fade_in.min(fade_out);
            material.emissive = LinearRgba::rgb(glow, 0.08 * glow, 0.08 * glow);
        }
    }

    if show.events.is_empty() {
        return;
    }
    show.elapsed += delta;
    while show.events.front().is_some_and(|event| event.at_secs <= show.elapsed) {
        let event = show.events.pop_front().expect("front checked above");
        match event.action {
            FireworkAction::Launch {
                pos,
                velocity,
                fuse_secs,
            } => {
                launch(&mut commands, &assets, pos, velocity, fuse_secs);
            }
            FireworkAction::Embers { pos, velocities } => {
                let shooter = Some(assets.my_player_id.0);
                for velocity in velocities {
                    spawn_ember_projectile(
                        &mut commands,
                        &assets.projectile_assets,
                        &vfx.gameplay_config,
                        pos,
                        velocity,
                        shooter,
                    );
                }
            }
            FireworkAction::LaserBeams { beams } => {
                spawn_laser_beams(&mut commands, &mut vfx, &assets, &beams);
            }
        }
    }
}

fn launch(commands: &mut Commands, assets: &FireworkAssets, pos: Vec3, velocity: Vec3, fuse_secs: f32) {
    commands
        .spawn((
            FireworkRocket { velocity, fuse_secs },
            Transform::from_translation(pos).with_rotation(missile_rotation(velocity)),
            Visibility::default(),
        ))
        .with_children(|parent| {
            spawn_missile_meshes(parent, &assets.missile_assets);
        });
    play_spatial_sound(
        commands,
        &assets.asset_server,
        assets.asset_set.player_sound("missile_launch"),
        &assets.client_settings.audio,
        pos,
    );
}

fn pop(commands: &mut Commands, vfx: &mut FireworkVfx, assets: &FireworkAssets, pos: Vec3) {
    let mut ctx = ExplosionSpawnCtx {
        meshes: &mut vfx.meshes,
        materials: &mut vfx.materials,
        budget: &mut vfx.budget,
        explosion_assets: &vfx.explosion_assets,
        gameplay_config: &vfx.gameplay_config,
        collision_world: &vfx.collision_world,
        map_layout: &vfx.map_layout,
        carriers: &vfx.carriers,
        carrier_entities: &vfx.carrier_entities,
        blast_radii: &vfx.blast_radii,
    };
    spawn_missile_explosion(commands, &mut ctx, Position::from(pos));
    play_explosion_sound(
        commands,
        &assets.asset_server,
        assets.asset_set.player_sound("explodes"),
        &assets.client_settings.audio,
        pos,
        Some(vfx.blast_radii.missile),
    );
}

fn spawn_laser_beams(commands: &mut Commands, vfx: &mut FireworkVfx, assets: &FireworkAssets, beams: &[LaserBeamSpec]) {
    // Same hot-red opaque-emissive recipe as the zapper beam — this app's
    // Blend materials render wrong, so bloom supplies the glow. The cylinder
    // is centered on the pivot, so the beam runs "to infinity" both ways.
    let mesh = vfx.meshes.add(Cylinder::new(BEAM_RADIUS, BEAM_LENGTH));
    for spec in beams {
        let material = vfx.materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.15, 0.15),
            // Starts dark; the fade-in envelope brings it up.
            emissive: LinearRgba::BLACK,
            ..default()
        });
        commands.spawn((
            FireworkLaser {
                spec: *spec,
                age_secs: 0.0,
                material: material.clone(),
            },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            NotShadowCaster,
            Transform::from_translation(spec.pivot).with_rotation(Quat::from_rotation_arc(Vec3::Y, spec.start_dir)),
            // Looping spatial hum from the pivot; despawning the beam stops it.
            sound_playback(
                &assets.asset_server,
                assets.asset_set.player_sound("laser_show"),
                PlaybackSettings::LOOP
                    .with_spatial(true)
                    .with_spatial_scale(SpatialScale::new(assets.client_settings.audio.spatial_distance_scale)),
            ),
        ));
    }
}

#[cfg(test)]
#[path = "tests/firework.rs"]
mod tests;
