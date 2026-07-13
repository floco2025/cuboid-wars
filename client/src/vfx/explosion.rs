use std::{
    collections::HashMap,
    f32::consts::{FRAC_PI_2, TAU},
};

use bevy::{
    asset::RenderAssetUsages, light::NotShadowCaster, mesh::Indices, prelude::*,
    render::render_resource::PrimitiveTopology,
};
use rand::{RngExt, SeedableRng, rng, rngs::SmallRng};

use common::{
    config::GameplayConfig,
    constants::WALL_HEIGHT,
    physics::{CollisionWorld, WorldSurfaceHit},
    protocol::Position,
};

use crate::constants::{
    EXPLOSION_FALLBACK_SCALE, EXPLOSION_FIREBALL_DIAMETER_FACTOR, EXPLOSION_FLASH_BRIGHTNESS,
    EXPLOSION_FLASH_LIFETIME_FACTOR, EXPLOSION_FLASH_START_ALPHA, EXPLOSION_LIFETIME_SECS, EXPLOSION_LIGHT_COLOR,
    EXPLOSION_LIGHT_INTENSITY, EXPLOSION_LIGHT_MIN_RANGE, EXPLOSION_LIGHT_RANGE_PER_RADIUS, EXPLOSION_RING_BRIGHTNESS,
    EXPLOSION_RING_DIAMETER_FACTOR, EXPLOSION_RING_LIFETIME_FACTOR, EXPLOSION_RING_RESOLUTION,
    EXPLOSION_RING_START_ALPHA, EXPLOSION_RING_THICKNESS, EXPLOSION_RING_Y_OFFSET, EXPLOSION_SCORCH_DIAMETER_FACTOR,
    EXPLOSION_SCORCH_FADE_SECS, EXPLOSION_SCORCH_LIFETIME_SECS, EXPLOSION_SCORCH_RESOLUTION,
    EXPLOSION_SCORCH_SURFACE_OFFSET, EXPLOSION_SHARD_BOUNCE_DAMPING, EXPLOSION_SHARD_BRIGHTNESS,
    EXPLOSION_SHARD_FRICTION, EXPLOSION_SHARD_GRAVITY, EXPLOSION_SHARD_LIFETIME_FACTOR, EXPLOSION_SHARD_MAX_COUNT,
    EXPLOSION_SHARD_MIN_COUNT, EXPLOSION_SHARD_SIZE, EXPLOSION_SHARD_SPEED_FACTOR, EXPLOSION_SHARD_UP_BIAS,
    EXPLOSION_SHARDS_PER_METER,
};

// Blast radii from `SInit` (per actor kind + the player death blast). Starts
// empty (initialized at app build) and is replaced when `Init` arrives;
// death cues can't arrive earlier — the pre-bootstrap dispatcher drops them.
#[derive(Resource, Default)]
pub struct ExplosionRadii {
    pub actors: HashMap<String, f32>,
    pub player: f32,
}

// Shared meshes plus material templates cloned for animated instances.
// Shards never fade, so one shared material serves every explosion.
const SCORCH_MESH_VARIANT_COUNT: usize = 12;

#[derive(Resource)]
pub struct ExplosionAssets {
    fireball_mesh: Handle<Mesh>,
    ring_mesh: Handle<Mesh>,
    scorch_meshes: Vec<Handle<Mesh>>,
    shard_mesh: Handle<Mesh>,
    shard_material: Handle<StandardMaterial>,
    fireball_template: StandardMaterial,
    ring_template: StandardMaterial,
    scorch_template: StandardMaterial,
}

impl ExplosionAssets {
    // Public (rather than folded into `FromWorld`) so tests can build the
    // resource against plain `Assets` collections.
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let flash = EXPLOSION_FLASH_BRIGHTNESS;
        let ring = EXPLOSION_RING_BRIGHTNESS;
        let shard = EXPLOSION_SHARD_BRIGHTNESS;
        Self {
            // Unit-diameter meshes: `Transform::scale` equals the layer's
            // world diameter in meters.
            fireball_mesh: meshes.add(Sphere::new(0.5)),
            ring_mesh: meshes.add(
                Annulus::new(0.5 - EXPLOSION_RING_THICKNESS, 0.5)
                    .mesh()
                    .resolution(EXPLOSION_RING_RESOLUTION)
                    .build(),
            ),
            scorch_meshes: (0..SCORCH_MESH_VARIANT_COUNT)
                .map(|variant| meshes.add(scorch_mesh(variant as u64)))
                .collect(),
            shard_mesh: meshes.add(Cuboid::new(
                EXPLOSION_SHARD_SIZE,
                EXPLOSION_SHARD_SIZE,
                EXPLOSION_SHARD_SIZE,
            )),
            shard_material: materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.6, 0.25),
                emissive: LinearRgba::rgb(shard, shard * 0.45, shard * 0.12),
                ..default()
            }),
            fireball_template: StandardMaterial {
                base_color: Color::srgba(1.0, 0.85, 0.6, EXPLOSION_FLASH_START_ALPHA),
                emissive: LinearRgba::rgb(flash, flash * 0.45, flash * 0.12),
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            ring_template: StandardMaterial {
                base_color: Color::srgba(1.0, 0.6, 0.3, EXPLOSION_RING_START_ALPHA),
                emissive: LinearRgba::rgb(ring, ring * 0.45, ring * 0.12),
                alpha_mode: AlphaMode::Blend,
                // The ring must render when seen from below a ledge or ramp.
                cull_mode: None,
                ..default()
            },
            scorch_template: StandardMaterial {
                base_color: Color::WHITE,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                unlit: true,
                depth_bias: 1.0,
                ..default()
            },
        }
    }
}

fn scorch_mesh(seed: u64) -> Mesh {
    const RING_RADII: [f32; 3] = [0.22, 0.39, 0.5];
    const RING_ALPHA: [f32; 3] = [0.84, 0.60, 0.0];
    const OUTLINE_CONTROL_POINTS: usize = 24;
    const DETAIL_CONTROL_POINTS: usize = 17;

    let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(0x5C0C_4A11));
    let outline: Vec<f32> = (0..OUTLINE_CONTROL_POINTS)
        .map(|_| rng.random_range(0.80..1.20))
        .collect();
    let ring_detail: Vec<Vec<f32>> = (0..RING_RADII.len())
        .map(|_| {
            (0..DETAIL_CONTROL_POINTS)
                .map(|_| rng.random_range(-0.04..0.04))
                .collect()
        })
        .collect();
    let alpha_detail: Vec<Vec<f32>> = (0..RING_RADII.len() - 1)
        .map(|_| {
            (0..DETAIL_CONTROL_POINTS)
                .map(|_| rng.random_range(-0.10..0.10))
                .collect()
        })
        .collect();

    let mut positions = Vec::with_capacity(1 + RING_RADII.len() * EXPLOSION_SCORCH_RESOLUTION);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut colors = Vec::with_capacity(positions.capacity());
    let mut indices = Vec::with_capacity(EXPLOSION_SCORCH_RESOLUTION * (3 + 6 * (RING_RADII.len() - 1)));

    positions.push([0.0, 0.0, 0.0]);
    normals.push([0.0, 1.0, 0.0]);
    colors.push(scorch_color(0.88, 0.0));

    for (ring_index, (&radius, &base_alpha)) in RING_RADII.iter().zip(&RING_ALPHA).enumerate() {
        for segment in 0..EXPLOSION_SCORCH_RESOLUTION {
            let progress = segment as f32 / EXPLOSION_SCORCH_RESOLUTION as f32;
            let angle = progress * TAU;
            let ring_radius = radius
                * (smooth_cyclic_sample(&outline, progress) + smooth_cyclic_sample(&ring_detail[ring_index], progress));
            let alpha_noise = if ring_index + 1 == RING_RADII.len() {
                0.0
            } else {
                smooth_cyclic_sample(&alpha_detail[ring_index], progress)
            };
            positions.push([ring_radius * angle.cos(), 0.0, ring_radius * angle.sin()]);
            normals.push([0.0, 1.0, 0.0]);
            colors.push(scorch_color(
                (base_alpha + alpha_noise).clamp(0.0, 1.0),
                ring_index as f32,
            ));
        }
    }

    for segment in 0..EXPLOSION_SCORCH_RESOLUTION {
        let current = 1 + segment as u32;
        let next = 1 + ((segment + 1) % EXPLOSION_SCORCH_RESOLUTION) as u32;
        indices.extend([0, next, current]);
    }

    for ring_index in 0..RING_RADII.len() - 1 {
        let inner_start = 1 + ring_index * EXPLOSION_SCORCH_RESOLUTION;
        let outer_start = inner_start + EXPLOSION_SCORCH_RESOLUTION;
        for segment in 0..EXPLOSION_SCORCH_RESOLUTION {
            let next = (segment + 1) % EXPLOSION_SCORCH_RESOLUTION;
            let inner = (inner_start + segment) as u32;
            let inner_next = (inner_start + next) as u32;
            let outer = (outer_start + segment) as u32;
            let outer_next = (outer_start + next) as u32;
            indices.extend([inner, outer_next, outer, inner, inner_next, outer_next]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn smooth_cyclic_sample(samples: &[f32], progress: f32) -> f32 {
    let sample_position = progress * samples.len() as f32;
    let current = sample_position.floor() as usize % samples.len();
    let next = (current + 1) % samples.len();
    let fraction = sample_position.fract();
    let smooth_fraction = fraction * fraction * (3.0 - 2.0 * fraction);
    samples[current] + (samples[next] - samples[current]) * smooth_fraction
}

fn scorch_color(alpha: f32, ring: f32) -> [f32; 4] {
    Color::srgba(0.035 + ring * 0.004, 0.022 + ring * 0.002, 0.012, alpha)
        .to_linear()
        .to_f32_array()
}

impl FromWorld for ExplosionAssets {
    fn from_world(world: &mut World) -> Self {
        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            Self::new(&mut meshes, &mut materials)
        })
    }
}

// Fireball flash and shockwave ring share one animation: ease-out scale
// growth plus alpha + emissive fade on a per-instance material clone.
#[derive(Component)]
pub struct ExplosionPulse {
    elapsed: f32,
    lifetime: f32,
    max_scale: f32,
    start_alpha: f32,
    // Template emissive at spawn; the fade rescales from this each frame.
    base_emissive: LinearRgba,
    material: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct ExplosionShard {
    velocity: Vec3,
    elapsed: f32,
    lifetime: f32,
    // The blast's floor plane — cosmetic bounce reference.
    floor_y: f32,
}

#[derive(Component)]
pub struct ExplosionLight {
    elapsed: f32,
    lifetime: f32,
    intensity: f32,
    range: f32,
}

#[derive(Component)]
pub struct ScorchMark {
    elapsed: f32,
    material: Handle<StandardMaterial>,
}

pub fn spawn_actor_explosion(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    explosion_assets: &ExplosionAssets,
    radii: &ExplosionRadii,
    gameplay_config: &GameplayConfig,
    collision_world: Option<&CollisionWorld>,
    actor_kind: &str,
    pos: Position,
) {
    let actor_physics = gameplay_config
        .actor(actor_kind)
        .expect("actor kind sent by server is missing from gameplay config")
        .physics();
    let blast_radius = radii.actors.get(actor_kind).copied();
    let fireball_diameter = blast_radius.map_or(EXPLOSION_FALLBACK_SCALE, |radius| {
        2.0 * radius * EXPLOSION_FIREBALL_DIAMETER_FACTOR
    });
    spawn_explosion(
        commands,
        materials,
        explosion_assets,
        Vec3::new(pos.x, actor_physics.collider_center_y(pos.y), pos.z),
        pos.y,
        fireball_diameter,
        blast_radius,
        collision_world,
    );
}

pub fn spawn_player_explosion(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    explosion_assets: &ExplosionAssets,
    radii: &ExplosionRadii,
    gameplay_config: &GameplayConfig,
    collision_world: Option<&CollisionWorld>,
    pos: Position,
) {
    let player_physics = gameplay_config.player.physics();
    spawn_explosion(
        commands,
        materials,
        explosion_assets,
        Vec3::new(pos.x, player_physics.collider_center_y(pos.y), pos.z),
        pos.y,
        2.0 * radii.player * EXPLOSION_FIREBALL_DIAMETER_FACTOR,
        Some(radii.player),
        collision_world,
    );
}

// Five layers: fireball flash, ground shockwave ring, scorch mark, debris
// shard burst, and a fading point light. `center` is the blast origin
// (collider center); `ground_y` anchors the ring at the victim's feet.
pub fn spawn_explosion(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    explosion_assets: &ExplosionAssets,
    center: Vec3,
    ground_y: f32,
    fireball_diameter: f32,
    blast_radius: Option<f32>,
    collision_world: Option<&CollisionWorld>,
) {
    // `None` = cosmetic burst with no area damage (unknown-kind fallback):
    // shards and light size off the fireball, and no ring is spawned — a
    // ring always marks a real danger area.
    let reach_radius = blast_radius.unwrap_or(fireball_diameter * 0.5);

    // Start pulses at a tiny scale, not zero — a degenerate scale inverts to
    // NaN normals for one frame.
    let fireball_material = materials.add(explosion_assets.fireball_template.clone());
    commands.spawn((
        Mesh3d(explosion_assets.fireball_mesh.clone()),
        MeshMaterial3d(fireball_material.clone()),
        NotShadowCaster,
        Transform::from_translation(center).with_scale(Vec3::splat(0.01)),
        ExplosionPulse {
            elapsed: 0.0,
            lifetime: EXPLOSION_LIFETIME_SECS * EXPLOSION_FLASH_LIFETIME_FACTOR,
            max_scale: fireball_diameter,
            start_alpha: EXPLOSION_FLASH_START_ALPHA,
            base_emissive: explosion_assets.fireball_template.emissive,
            material: fireball_material,
        },
    ));

    if let Some(blast_radius) = blast_radius {
        let ring_material = materials.add(explosion_assets.ring_template.clone());
        commands.spawn((
            Mesh3d(explosion_assets.ring_mesh.clone()),
            MeshMaterial3d(ring_material.clone()),
            NotShadowCaster,
            Transform {
                translation: Vec3::new(center.x, ground_y + EXPLOSION_RING_Y_OFFSET, center.z),
                // The annulus meshes in the XY plane; lay it flat on XZ.
                rotation: Quat::from_rotation_x(-FRAC_PI_2),
                scale: Vec3::splat(0.01),
            },
            ExplosionPulse {
                elapsed: 0.0,
                lifetime: EXPLOSION_LIFETIME_SECS * EXPLOSION_RING_LIFETIME_FACTOR,
                max_scale: 2.0 * blast_radius * EXPLOSION_RING_DIAMETER_FACTOR,
                start_alpha: EXPLOSION_RING_START_ALPHA,
                base_emissive: explosion_assets.ring_template.emissive,
                material: ring_material,
            },
        ));
    }

    let mut rng = rng();
    let scorch_diameter = 2.0 * reach_radius * EXPLOSION_SCORCH_DIAMETER_FACTOR;
    if let Some(surface) = collision_world.and_then(|world| {
        let standing_distance = (center.y - ground_y).max(0.0) + EXPLOSION_SCORCH_SURFACE_OFFSET;
        world.ground_surface_below(center, reach_radius.max(standing_distance))
    }) {
        spawn_scorch_mark(
            commands,
            materials,
            explosion_assets,
            surface,
            scorch_diameter,
            &mut rng,
        );
    }
    if let Some(world) = collision_world {
        let wall_scorch_diameter = scorch_diameter.min(WALL_HEIGHT);
        for direction in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
            if let Some(surface) = world.wall_surface_along_ray(center, direction, reach_radius) {
                spawn_scorch_mark(
                    commands,
                    materials,
                    explosion_assets,
                    surface,
                    wall_scorch_diameter,
                    &mut rng,
                );
            }
        }
    }

    let shard_lifetime = EXPLOSION_LIFETIME_SECS * EXPLOSION_SHARD_LIFETIME_FACTOR;
    for _ in 0..shard_count(reach_radius) {
        let direction = (Vec3::new(
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
        ) + Vec3::Y * EXPLOSION_SHARD_UP_BIAS)
            .normalize_or_zero();
        let direction = if direction == Vec3::ZERO { Vec3::Y } else { direction };
        let speed = reach_radius / shard_lifetime * EXPLOSION_SHARD_SPEED_FACTOR * rng.random_range(0.7..1.3);
        commands.spawn((
            Mesh3d(explosion_assets.shard_mesh.clone()),
            MeshMaterial3d(explosion_assets.shard_material.clone()),
            NotShadowCaster,
            Transform::from_translation(center),
            ExplosionShard {
                velocity: direction * speed,
                elapsed: 0.0,
                lifetime: shard_lifetime,
                floor_y: ground_y,
            },
        ));
    }

    // Own entity: the light fades over the full master lifetime, outliving
    // the shorter fireball flash.
    let range = (reach_radius * EXPLOSION_LIGHT_RANGE_PER_RADIUS).max(EXPLOSION_LIGHT_MIN_RANGE);
    commands.spawn((
        PointLight {
            color: EXPLOSION_LIGHT_COLOR,
            intensity: EXPLOSION_LIGHT_INTENSITY,
            range,
            radius: 1.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(center),
        ExplosionLight {
            elapsed: 0.0,
            lifetime: EXPLOSION_LIFETIME_SECS,
            intensity: EXPLOSION_LIGHT_INTENSITY,
            range,
        },
    ));
}

fn spawn_scorch_mark(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    explosion_assets: &ExplosionAssets,
    surface: WorldSurfaceHit,
    diameter: f32,
    rng: &mut impl rand::Rng,
) {
    let material = materials.add(explosion_assets.scorch_template.clone());
    let alignment = Quat::from_rotation_arc(Vec3::Y, surface.normal);
    let random_rotation = Quat::from_axis_angle(surface.normal, rng.random_range(0.0..TAU));
    let scorch_mesh = explosion_assets.scorch_meshes[rng.random_range(0..explosion_assets.scorch_meshes.len())].clone();
    commands.spawn((
        Mesh3d(scorch_mesh),
        MeshMaterial3d(material.clone()),
        NotShadowCaster,
        Transform {
            translation: surface.point + surface.normal * EXPLOSION_SCORCH_SURFACE_OFFSET,
            rotation: random_rotation * alignment,
            scale: Vec3::splat(diameter),
        },
        ScorchMark { elapsed: 0.0, material },
    ));
}

fn scorch_alpha(elapsed: f32) -> f32 {
    ((EXPLOSION_SCORCH_LIFETIME_SECS - elapsed) / EXPLOSION_SCORCH_FADE_SECS).clamp(0.0, 1.0)
}

pub fn scorch_marks_system(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut marks: Query<(Entity, &mut ScorchMark)>,
) {
    let delta = time.delta_secs();
    for (entity, mut mark) in &mut marks {
        mark.elapsed += delta;
        if mark.elapsed >= EXPLOSION_SCORCH_LIFETIME_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(mut material) = materials.get_mut(&mark.material) {
            material.base_color.set_alpha(scorch_alpha(mark.elapsed));
        }
    }
}

fn shard_count(reach_radius: f32) -> usize {
    ((reach_radius * EXPLOSION_SHARDS_PER_METER).ceil() as usize)
        .clamp(EXPLOSION_SHARD_MIN_COUNT, EXPLOSION_SHARD_MAX_COUNT)
}

pub fn explosion_pulse_system(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut pulses: Query<(Entity, &mut ExplosionPulse, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (entity, mut pulse, mut transform) in &mut pulses {
        pulse.elapsed += delta;
        // The per-instance material asset frees itself when the entity drops
        // the last handle to it.
        if pulse.elapsed >= pulse.lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        let progress = (pulse.elapsed / pulse.lifetime).clamp(0.0, 1.0);
        let grow = 1.0 - (1.0 - progress).powi(3);
        transform.scale = Vec3::splat(pulse.max_scale * grow);
        if let Some(mut material) = materials.get_mut(&pulse.material) {
            material.base_color.set_alpha(pulse.start_alpha * (1.0 - progress));
            // Emissive has no ceiling, so the alpha fade alone can't pull
            // extreme brightness values under the bloom threshold before
            // despawn — square-fade the emissive too so the glow dies
            // smoothly instead of blinking out.
            material.emissive = pulse.base_emissive * (1.0 - progress).powi(2);
        }
    }
}

// Ballistic fade-out, like bounce sparks but with per-explosion lifetimes.
pub fn explosion_shards_system(
    mut commands: Commands,
    time: Res<Time>,
    mut shards: Query<(Entity, &mut ExplosionShard, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (entity, mut shard, mut transform) in &mut shards {
        shard.elapsed += delta;
        if shard.elapsed >= shard.lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        shard.velocity.y -= EXPLOSION_SHARD_GRAVITY * delta;
        transform.translation += shard.velocity * delta;
        let floor = shard.floor_y + EXPLOSION_SHARD_SIZE * 0.5;
        if transform.translation.y < floor && shard.velocity.y < 0.0 {
            transform.translation.y = floor;
            shard.velocity.y = -shard.velocity.y * EXPLOSION_SHARD_BOUNCE_DAMPING;
            shard.velocity.x *= EXPLOSION_SHARD_FRICTION;
            shard.velocity.z *= EXPLOSION_SHARD_FRICTION;
        }
        transform.scale = Vec3::splat(1.0 - shard.elapsed / shard.lifetime);
    }
}

pub fn explosion_lights_system(
    mut commands: Commands,
    time: Res<Time>,
    mut lights: Query<(Entity, &mut ExplosionLight, &mut PointLight)>,
) {
    let delta = time.delta_secs();
    for (entity, mut state, mut light) in &mut lights {
        state.elapsed += delta;
        if state.elapsed >= state.lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        let progress = (state.elapsed / state.lifetime).clamp(0.0, 1.0);
        let fade = (1.0 - progress).powi(2);
        light.intensity = state.intensity * fade;
        light.range = state.range * fade.max(0.25);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{EXPLOSION_SHARD_MAX_COUNT, EXPLOSION_SHARD_MIN_COUNT};
    use common::protocol::{BarrierKindTable, Floor, MapLayout, Wall};

    #[test]
    fn shard_count_clamps_to_bounds() {
        assert_eq!(shard_count(0.1), EXPLOSION_SHARD_MIN_COUNT);
        assert_eq!(shard_count(1000.0), EXPLOSION_SHARD_MAX_COUNT);
        let mid = shard_count(10.0);
        assert!(mid > EXPLOSION_SHARD_MIN_COUNT && mid < EXPLOSION_SHARD_MAX_COUNT);
    }

    #[test]
    fn shard_count_steps_up_with_blast_radius() {
        // The three shipped kinds must be visibly distinct.
        assert!(shard_count(6.0) < shard_count(10.0));
        assert!(shard_count(10.0) < shard_count(15.0));
    }

    #[test]
    fn scorch_alpha_stays_opaque_then_fades_to_zero() {
        assert_eq!(scorch_alpha(0.0), 1.0);
        assert_eq!(
            scorch_alpha(EXPLOSION_SCORCH_LIFETIME_SECS - EXPLOSION_SCORCH_FADE_SECS),
            1.0
        );
        assert_eq!(
            scorch_alpha(EXPLOSION_SCORCH_LIFETIME_SECS - EXPLOSION_SCORCH_FADE_SECS / 2.0),
            0.5
        );
        assert_eq!(scorch_alpha(EXPLOSION_SCORCH_LIFETIME_SECS), 0.0);
    }

    #[test]
    fn grounded_explosion_spawns_one_sized_scorch_mark() {
        let collision_world = CollisionWorld::from_map_layout(
            &MapLayout {
                floors: vec![Floor {
                    x1: -10.0,
                    z1: -10.0,
                    x2: 10.0,
                    z2: 10.0,
                    y: 0.0,
                    thickness: 1.0,
                    level: 0,
                }],
                ..default()
            },
            &BarrierKindTable::default(),
        );
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let explosion_assets = ExplosionAssets::new(&mut meshes, &mut materials);
        let mut world = World::new();
        let mut queue = bevy::ecs::world::CommandQueue::default();

        {
            let mut commands = Commands::new(&mut queue, &world);
            spawn_explosion(
                &mut commands,
                &mut materials,
                &explosion_assets,
                Vec3::new(0.0, 1.0, 0.0),
                0.0,
                2.0,
                Some(2.0),
                Some(&collision_world),
            );
        }
        queue.apply(&mut world);

        let mut marks = world.query::<(&ScorchMark, &Transform)>();
        let marks: Vec<_> = marks.iter(&world).collect();
        assert_eq!(marks.len(), 1);
        let transform = marks[0].1;
        assert!((transform.translation.y - EXPLOSION_SCORCH_SURFACE_OFFSET).abs() < 0.001);
        assert_eq!(
            transform.scale,
            Vec3::splat(2.0 * 2.0 * EXPLOSION_SCORCH_DIAMETER_FACTOR)
        );
    }

    #[test]
    fn explosion_next_to_wall_spawns_wall_scorch_mark() {
        let collision_world = CollisionWorld::from_map_layout(
            &MapLayout {
                walls: vec![Wall {
                    x1: -10.0,
                    z1: 1.0,
                    x2: 10.0,
                    z2: 1.0,
                    width: 0.2,
                    level: 0,
                }],
                floors: vec![Floor {
                    x1: -10.0,
                    z1: -10.0,
                    x2: 10.0,
                    z2: 10.0,
                    y: 0.0,
                    thickness: 1.0,
                    level: 0,
                }],
                ..default()
            },
            &BarrierKindTable::default(),
        );
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let explosion_assets = ExplosionAssets::new(&mut meshes, &mut materials);
        let mut world = World::new();
        let mut queue = bevy::ecs::world::CommandQueue::default();

        {
            let mut commands = Commands::new(&mut queue, &world);
            spawn_explosion(
                &mut commands,
                &mut materials,
                &explosion_assets,
                Vec3::new(0.0, 1.0, 0.0),
                0.0,
                6.0,
                Some(6.0),
                Some(&collision_world),
            );
        }
        queue.apply(&mut world);

        let mut marks = world.query::<(&ScorchMark, &Transform)>();
        let marks: Vec<_> = marks.iter(&world).collect();
        assert_eq!(marks.len(), 2);
        let wall_transform = marks
            .iter()
            .find_map(|(_, transform)| {
                let surface_normal = transform.rotation * Vec3::Y;
                (surface_normal.dot(Vec3::NEG_Z) > 0.999).then_some(*transform)
            })
            .expect("expected wall-aligned scorch mark");
        assert_eq!(wall_transform.scale, Vec3::splat(WALL_HEIGHT));
    }
}
