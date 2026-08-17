use std::collections::VecDeque;
use std::f32::consts::TAU;

use bevy::{audio::SpatialScale, ecs::system::SystemParam, prelude::*};
use rand::{RngExt, SeedableRng, rngs::StdRng};

use crate::{
    audio::{play_explosion_sound, play_spatial_sound},
    config::{AssetSet, ClientSettings},
    constants::LASER_EMISSIVE,
    missiles::{MissileAssets, missile_rotation, spawn_missile_meshes},
    players::MyPlayerId,
    projectiles::{ProjectileAssets, spawn_ember_projectile},
    vfx::{ExplosionAssets, ExplosionSpawnCtx, ExplosionVfxBudget, spawn_missile_explosion},
};
use common::{
    config::GameplayConfig,
    constants::LEVEL_HEIGHT,
    physics::CollisionWorld,
    protocol::{MapLayout, Position},
};

// ============================================================================
// Show shape
// ============================================================================
// The whole ~26 s choreography is derived up front from the broadcast seed,
// so every client plays an identical show. All randomness is resolved at
// build time; playback is deterministic.

const ROCKET_SPEED: f32 = 22.0;
const STAR_SPEED: f32 = 10.0;
const STAR_FUSE_SECS: f32 = 0.55;
// Launch ring sits outside the map footprint and below the ground floor.
const RING_MARGIN: f32 = 8.0;
const ORIGIN_DEPTH_Y: f32 = -8.0;
// Pops happen this far above the tallest floor level (plus up to
// SKY_JITTER more), so blasts are pure sky decoration.
const SKY_CLEARANCE: f32 = 12.0;
const SKY_JITTER: f32 = 8.0;
const EMBERS_PER_POP: usize = 14;
// Sky lasers: beams long enough to read as infinite, pivoting on ring
// points and sweeping across the sky.
const BEAM_LENGTH: f32 = 1200.0;
const BEAM_RADIUS: f32 = 0.12;
const BEAM_FADE_SECS: f32 = 0.6;
// Half-extent fallback when the show starts before the map arrived.
const FALLBACK_HALF_EXTENT: f32 = 40.0;

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
    pub fn start(&mut self, seed: u64, map_layout: Option<&MapLayout>) {
        self.elapsed = 0.0;
        self.events = build_show(seed, map_layout);
    }
}

// A cosmetic rocket in flight. Deliberately NOT a `MissileMarker` entity —
// the missile snapshot diff / transform sync / dead reckoning must never see
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

struct ShowField {
    center: Vec3,
    half_x: f32,
    half_z: f32,
    sky_base: f32,
    ring_radius: f32,
}

fn show_field(map_layout: Option<&MapLayout>) -> ShowField {
    let (min_x, max_x, min_z, max_z, max_level) = map_layout
        .filter(|layout| !layout.floors.is_empty())
        .map(|layout| {
            let mut min_x = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut min_z = f32::INFINITY;
            let mut max_z = f32::NEG_INFINITY;
            let mut max_level = 0u8;
            for floor in &layout.floors {
                let (x1, x2, z1, z2) = (floor.x1, floor.x2, floor.z1, floor.z2);
                min_x = min_x.min(x1.min(x2));
                max_x = max_x.max(x1.max(x2));
                min_z = min_z.min(z1.min(z2));
                max_z = max_z.max(z1.max(z2));
                max_level = max_level.max(floor.level);
            }
            (min_x, max_x, min_z, max_z, max_level)
        })
        .unwrap_or((
            -FALLBACK_HALF_EXTENT,
            FALLBACK_HALF_EXTENT,
            -FALLBACK_HALF_EXTENT,
            FALLBACK_HALF_EXTENT,
            5,
        ));
    let half_x = (max_x - min_x) / 2.0;
    let half_z = (max_z - min_z) / 2.0;
    ShowField {
        center: Vec3::new((min_x + max_x) / 2.0, 0.0, (min_z + max_z) / 2.0),
        half_x,
        half_z,
        sky_base: f32::from(max_level + 1) * LEVEL_HEIGHT + SKY_CLEARANCE,
        ring_radius: half_x.hypot(half_z) + RING_MARGIN,
    }
}

fn build_show(seed: u64, map_layout: Option<&MapLayout>) -> VecDeque<FireworkEvent> {
    let field = show_field(map_layout);
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
// at a random sky point over the field; the fuse is the exact flight time,
// so the pop lands on the aimed point. Optional star burst and laser spokes
// are scheduled at that precomputed pop.
fn rocket(events: &mut Vec<FireworkEvent>, rng: &mut StdRng, field: &ShowField, at_secs: f32, stars: bool) {
    let ring_angle = rng.random_range(0.0..TAU);
    let origin = field.center
        + Vec3::new(
            ring_angle.cos() * field.ring_radius,
            ORIGIN_DEPTH_Y,
            ring_angle.sin() * field.ring_radius,
        );
    let sky = field.center
        + Vec3::new(
            rng.random_range(-0.6..0.6) * field.half_x,
            field.sky_base + rng.random_range(0.0..SKY_JITTER),
            rng.random_range(-0.6..0.6) * field.half_z,
        );
    let flight = sky - origin;
    let fuse_secs = flight.length() / ROCKET_SPEED;
    events.push(FireworkEvent {
        at_secs,
        action: FireworkAction::Launch {
            pos: origin,
            velocity: flight.normalize() * ROCKET_SPEED,
            fuse_secs,
        },
    });

    let pop_at = at_secs + fuse_secs;
    if stars {
        for _ in 0..7 {
            let dir = random_unit(rng);
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
    let pivot = field.center
        + Vec3::new(
            ring_angle.cos() * field.ring_radius,
            0.0,
            ring_angle.sin() * field.ring_radius,
        );
    // Tilt from vertical: 25°..65° — always aimed over the field, never flat
    // into buildings.
    let tilt: f32 = rng.random_range(0.44..1.13);
    let toward = (field.center - pivot).normalize_or_zero();
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

fn random_unit(rng: &mut StdRng) -> Vec3 {
    loop {
        let v = Vec3::new(
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
        );
        let len_sq = v.length_squared();
        if len_sq > 0.01 && len_sq <= 1.0 {
            return v / len_sq.sqrt();
        }
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
    gameplay_config: Res<'w, GameplayConfig>,
    collision_world: Option<Res<'w, CollisionWorld>>,
    map_layout: Option<Res<'w, MapLayout>>,
}

#[derive(SystemParam)]
pub struct FireworkAssets<'w> {
    asset_server: Res<'w, AssetServer>,
    asset_set: Res<'w, AssetSet>,
    client_settings: Res<'w, ClientSettings>,
    missile_assets: Res<'w, MissileAssets>,
    projectile_assets: Res<'w, ProjectileAssets>,
    my_player_id: Option<Res<'w, MyPlayerId>>,
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
                let shooter = assets.my_player_id.as_ref().map(|id| id.0);
                for velocity in velocities {
                    spawn_ember_projectile(
                        &mut commands,
                        &assets.projectile_assets,
                        &vfx.gameplay_config.projectiles,
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
        collision_world: vfx.collision_world.as_deref(),
        map_layout: vfx.map_layout.as_deref(),
    };
    spawn_missile_explosion(commands, &mut ctx, Position::from(pos));
    play_explosion_sound(
        commands,
        &assets.asset_server,
        assets.asset_set.player_sound("explodes"),
        &assets.client_settings.audio,
        pos,
        Some(vfx.gameplay_config.missiles.blast_radius),
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
            Transform::from_translation(spec.pivot).with_rotation(Quat::from_rotation_arc(Vec3::Y, spec.start_dir)),
            // Looping spatial hum from the pivot; despawning the beam stops it.
            AudioPlayer::new(
                assets
                    .asset_server
                    .load(assets.asset_set.player_sound("laser_show").to_owned()),
            ),
            PlaybackSettings::LOOP
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new(assets.client_settings.audio.spatial_distance_scale)),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::Floor;

    fn layout() -> MapLayout {
        MapLayout {
            floors: vec![
                Floor {
                    x1: -20.0,
                    z1: -15.0,
                    x2: 20.0,
                    z2: 15.0,
                    y: 0.0,
                    thickness: 0.4,
                    level: 0,
                },
                Floor {
                    x1: -5.0,
                    z1: -5.0,
                    x2: 5.0,
                    z2: 5.0,
                    y: 3.0 * LEVEL_HEIGHT,
                    thickness: 0.4,
                    level: 3,
                },
            ],
            ..Default::default()
        }
    }

    fn positions(events: &VecDeque<FireworkEvent>) -> Vec<(f32, Vec3)> {
        events
            .iter()
            .map(|event| match &event.action {
                FireworkAction::Launch { pos, .. } | FireworkAction::Embers { pos, .. } => (event.at_secs, *pos),
                FireworkAction::LaserBeams { beams } => (event.at_secs, beams[0].pivot),
            })
            .collect()
    }

    #[test]
    fn same_seed_builds_the_identical_show() {
        let layout = layout();
        let a = build_show(42, Some(&layout));
        let b = build_show(42, Some(&layout));
        assert_eq!(positions(&a), positions(&b), "cross-client sync relies on determinism");
        assert!(!a.is_empty());
    }

    #[test]
    fn events_are_time_sorted() {
        let show = build_show(7, Some(&layout()));
        let times: Vec<f32> = show.iter().map(|event| event.at_secs).collect();
        let mut sorted = times.clone();
        sorted.sort_by(f32::total_cmp);
        assert_eq!(times, sorted);
    }

    #[test]
    fn rockets_launch_outside_and_below_and_pop_safely_high() {
        let layout = layout();
        let field = show_field(Some(&layout));
        let show = build_show(123, Some(&layout));
        for event in &show {
            match &event.action {
                // Ground launches (fuse > star fuse) start outside the
                // footprint and below the ground floor; star second stages
                // start at sky height instead.
                FireworkAction::Launch { pos, fuse_secs, .. } if *fuse_secs > STAR_FUSE_SECS => {
                    assert!(pos.y < 0.0, "launch origin above ground: {pos}");
                    let planar = Vec3::new(pos.x - field.center.x, 0.0, pos.z - field.center.z).length();
                    assert!(
                        planar > field.half_x.max(field.half_z),
                        "launch origin inside footprint: {pos}"
                    );
                }
                FireworkAction::Launch { pos, .. } | FireworkAction::Embers { pos, .. } => {
                    assert!(
                        pos.y >= field.sky_base,
                        "sky event below safe height: {pos} (base {})",
                        field.sky_base
                    );
                }
                FireworkAction::LaserBeams { beams } => {
                    for beam in beams {
                        let planar =
                            Vec3::new(beam.pivot.x - field.center.x, 0.0, beam.pivot.z - field.center.z).length();
                        assert!(planar > field.half_x.max(field.half_z), "beam pivot inside footprint");
                        assert!(beam.start_dir.y > 0.4, "beam not aimed skyward: {}", beam.start_dir);
                    }
                }
            }
        }
    }
}
