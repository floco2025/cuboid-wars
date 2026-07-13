use std::collections::VecDeque;

use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use rand::{Rng, RngExt};

use crate::{
    cameras::MainCameraMarker,
    config::{ClientSettings, ExplosionVfxConfig},
    constants::*,
};
use common::physics::{CollisionWorld, WorldSurfaceHit};

const CUBE_VERTICES: [Vec3; 24] = [
    Vec3::new(-0.5, -0.5, 0.5),
    Vec3::new(0.5, -0.5, 0.5),
    Vec3::new(0.5, 0.5, 0.5),
    Vec3::new(-0.5, 0.5, 0.5),
    Vec3::new(0.5, -0.5, -0.5),
    Vec3::new(-0.5, -0.5, -0.5),
    Vec3::new(-0.5, 0.5, -0.5),
    Vec3::new(0.5, 0.5, -0.5),
    Vec3::new(0.5, -0.5, 0.5),
    Vec3::new(0.5, -0.5, -0.5),
    Vec3::new(0.5, 0.5, -0.5),
    Vec3::new(0.5, 0.5, 0.5),
    Vec3::new(-0.5, -0.5, -0.5),
    Vec3::new(-0.5, -0.5, 0.5),
    Vec3::new(-0.5, 0.5, 0.5),
    Vec3::new(-0.5, 0.5, -0.5),
    Vec3::new(-0.5, 0.5, 0.5),
    Vec3::new(0.5, 0.5, 0.5),
    Vec3::new(0.5, 0.5, -0.5),
    Vec3::new(-0.5, 0.5, -0.5),
    Vec3::new(-0.5, -0.5, -0.5),
    Vec3::new(0.5, -0.5, -0.5),
    Vec3::new(0.5, -0.5, 0.5),
    Vec3::new(-0.5, -0.5, 0.5),
];
const CUBE_NORMALS: [Vec3; 24] = [
    Vec3::Z,
    Vec3::Z,
    Vec3::Z,
    Vec3::Z,
    Vec3::NEG_Z,
    Vec3::NEG_Z,
    Vec3::NEG_Z,
    Vec3::NEG_Z,
    Vec3::X,
    Vec3::X,
    Vec3::X,
    Vec3::X,
    Vec3::NEG_X,
    Vec3::NEG_X,
    Vec3::NEG_X,
    Vec3::NEG_X,
    Vec3::Y,
    Vec3::Y,
    Vec3::Y,
    Vec3::Y,
    Vec3::NEG_Y,
    Vec3::NEG_Y,
    Vec3::NEG_Y,
    Vec3::NEG_Y,
];
const CUBE_INDICES: [u32; 36] = [
    0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16, 17, 18, 16, 18, 19, 20, 21,
    22, 20, 22, 23,
];
const SMOKE_RING_SEGMENTS: usize = 12;
const SMOKE_INNER_RADIUS: f32 = 0.45;
const SMOKE_VERTICES_PER_PARTICLE: usize = 1 + SMOKE_RING_SEGMENTS * 2;

#[derive(Resource, Default)]
pub struct ExplosionVfxBudget {
    active_shards: usize,
    active_smoke: usize,
    active_lights: usize,
    scorches: VecDeque<Entity>,
}

impl ExplosionVfxBudget {
    fn reserve_shards(&mut self, requested: usize, max_active: usize) -> usize {
        let granted = requested.min(max_active.saturating_sub(self.active_shards));
        self.active_shards += granted;
        granted
    }

    fn release_shards(&mut self, count: usize) {
        self.active_shards = self.active_shards.saturating_sub(count);
    }

    fn reserve_smoke(&mut self, requested: usize, max_active: usize) -> usize {
        let granted = requested.min(max_active.saturating_sub(self.active_smoke));
        self.active_smoke += granted;
        granted
    }

    fn release_smoke(&mut self, count: usize) {
        self.active_smoke = self.active_smoke.saturating_sub(count);
    }

    pub(super) fn reserve_light(&mut self, max_active: usize) -> bool {
        if self.active_lights >= max_active {
            return false;
        }
        self.active_lights += 1;
        true
    }

    pub(super) fn release_light(&mut self) {
        self.active_lights = self.active_lights.saturating_sub(1);
    }

    pub(super) fn register_scorch(&mut self, commands: &mut Commands, entity: Entity, max_active: usize) {
        if self.scorches.len() >= max_active
            && let Some(oldest) = self.scorches.pop_front()
        {
            commands.entity(oldest).despawn();
        }
        self.scorches.push_back(entity);
    }

    pub(super) fn remove_scorch(&mut self, entity: Entity) {
        self.scorches.retain(|candidate| *candidate != entity);
    }
}

#[derive(Clone, Copy)]
pub(super) struct SurfacePlane {
    point: Vec3,
    normal: Vec3,
    radius: f32,
}

impl SurfacePlane {
    pub(super) fn from_hit(hit: WorldSurfaceHit, center: Vec3, radius: f32) -> Self {
        Self {
            point: hit.point - center,
            normal: hit.normal,
            radius,
        }
    }
}

struct ShardParticle {
    position: Vec3,
    velocity: Vec3,
    rotation: Quat,
    angular_velocity: Vec3,
    size: f32,
    lifetime: f32,
    travelled: f32,
    max_distance: Option<f32>,
    color: [f32; 4],
}

#[derive(Component)]
pub struct ExplosionShardCloud {
    particles: Vec<ShardParticle>,
    elapsed: f32,
    mesh: Handle<Mesh>,
    ground: Option<SurfacePlane>,
    reserved_count: usize,
}

struct SmokeParticle {
    position: Vec3,
    velocity: Vec3,
    rotation: f32,
    angular_velocity: f32,
    aspect: Vec2,
    start_size: f32,
    end_size: f32,
    lifetime: f32,
    color: Vec3,
}

#[derive(Component)]
pub struct ExplosionSmokeCloud {
    particles: Vec<SmokeParticle>,
    elapsed: f32,
    mesh: Handle<Mesh>,
    reserved_count: usize,
}

#[expect(
    clippy::too_many_arguments,
    reason = "particle cloud spawn receives its render and surface context"
)]
pub(super) fn spawn_shard_cloud(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    collision_world: Option<&CollisionWorld>,
    ground: Option<SurfacePlane>,
    center: Vec3,
    reach_radius: f32,
    requested_count: usize,
    config: &ExplosionVfxConfig,
    rng: &mut impl Rng,
) {
    let count = budget.reserve_shards(requested_count, EXPLOSION_SHARD_GLOBAL_MAX_COUNT);
    if count == 0 {
        return;
    }
    let base_lifetime = config.base_duration_secs * EXPLOSION_SHARD_LIFETIME_FACTOR;
    let mut particles = Vec::with_capacity(count);
    for _ in 0..count {
        let direction = random_direction(rng);
        let lifetime = base_lifetime;
        let speed = reach_radius / base_lifetime * EXPLOSION_SHARD_SPEED_FACTOR * rng.random_range(0.7..1.3);
        let max_distance = collision_world
            .and_then(|world| world.wall_surface_along_ray(center, direction, reach_radius))
            .map(|surface| center.distance(surface.point));
        particles.push(ShardParticle {
            position: Vec3::ZERO,
            velocity: direction * speed,
            rotation: Quat::from_euler(
                EulerRot::XYZ,
                rng.random_range(0.0..std::f32::consts::TAU),
                rng.random_range(0.0..std::f32::consts::TAU),
                rng.random_range(0.0..std::f32::consts::TAU),
            ),
            angular_velocity: Vec3::new(
                rng.random_range(-8.0..8.0),
                rng.random_range(-8.0..8.0),
                rng.random_range(-8.0..8.0),
            ),
            size: config.shards.size,
            lifetime,
            travelled: 0.0,
            max_distance,
            color: LinearRgba::WHITE.to_f32_array(),
        });
    }
    let mesh = meshes.add(particle_mesh(&particles, 0.0));
    commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material),
        NotShadowCaster,
        Transform::from_translation(center),
        ExplosionShardCloud {
            particles,
            elapsed: 0.0,
            mesh,
            ground,
            reserved_count: count,
        },
    ));
}

pub(super) fn spawn_smoke_cloud(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    center: Vec3,
    reach_radius: f32,
    requested_count: usize,
    config: &ExplosionVfxConfig,
    rng: &mut impl Rng,
) {
    let count = budget.reserve_smoke(requested_count, EXPLOSION_SMOKE_GLOBAL_MAX_COUNT);
    if count == 0 {
        return;
    }
    let mut particles = Vec::with_capacity(count);
    for _ in 0..count {
        let horizontal = Vec3::new(rng.random_range(-1.0..1.0), 0.0, rng.random_range(-1.0..1.0));
        let offset = horizontal.normalize_or_zero() * rng.random_range(0.0..reach_radius * 0.12);
        let shade = rng.random_range(0.20..0.36);
        particles.push(SmokeParticle {
            position: offset,
            velocity: horizontal * rng.random_range(0.15..0.55) + Vec3::Y * rng.random_range(0.45..1.15),
            rotation: rng.random_range(0.0..std::f32::consts::TAU),
            angular_velocity: rng.random_range(-0.5..0.5),
            aspect: Vec2::new(rng.random_range(0.85..1.15), rng.random_range(0.80..1.10)),
            start_size: EXPLOSION_SMOKE_START_SIZE * rng.random_range(0.7..1.25),
            end_size: config.smoke.end_size * rng.random_range(0.75..1.35),
            lifetime: config.smoke.lifetime_secs * rng.random_range(0.9..1.15),
            color: Vec3::new(shade * 1.08, shade, shade * 0.9),
        });
    }
    let mesh = meshes.add(smoke_mesh(
        &particles,
        0.0,
        Vec3::X,
        Vec3::Y,
        Vec3::Z,
        config.smoke.max_opacity,
    ));
    commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material),
        NotShadowCaster,
        Transform::from_translation(center),
        ExplosionSmokeCloud {
            particles,
            elapsed: 0.0,
            mesh,
            reserved_count: count,
        },
    ));
}

pub fn explosion_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<ClientSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut budget: ResMut<ExplosionVfxBudget>,
    mut shards: Query<(Entity, &mut ExplosionShardCloud)>,
    mut smoke: Query<(Entity, &mut ExplosionSmokeCloud)>,
    main_camera: Query<&GlobalTransform, (With<Camera3d>, With<MainCameraMarker>)>,
) {
    let delta = time.delta_secs();
    let config = &settings.vfx.explosions;
    for (entity, mut cloud) in &mut shards {
        cloud.elapsed += delta;
        let elapsed = cloud.elapsed;
        let ground = cloud.ground;
        let mut alive = false;
        for particle in &mut cloud.particles {
            if elapsed >= particle.lifetime || particle.max_distance.is_some_and(|limit| particle.travelled >= limit) {
                continue;
            }
            alive = true;
            particle.velocity.y -= EXPLOSION_SHARD_GRAVITY * delta;
            let step = particle.velocity * delta;
            particle.position += step;
            particle.travelled += step.length();
            particle.rotation = Quat::from_scaled_axis(particle.angular_velocity * delta) * particle.rotation;
            if let Some(plane) = ground {
                bounce_on_surface(particle, plane);
            }
        }
        if !alive {
            budget.release_shards(cloud.reserved_count);
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(mut mesh) = meshes.get_mut(&cloud.mesh) {
            update_particle_mesh(&mut mesh, &cloud.particles, elapsed);
        }
    }

    let (smoke_right, smoke_up, smoke_normal) = main_camera.single().map_or((Vec3::X, Vec3::Y, Vec3::Z), |camera| {
        let rotation = camera.to_scale_rotation_translation().1;
        (rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z)
    });
    for (entity, mut cloud) in &mut smoke {
        cloud.elapsed += delta;
        let elapsed = cloud.elapsed;
        let mut alive = false;
        for particle in &mut cloud.particles {
            if elapsed >= particle.lifetime {
                continue;
            }
            alive = true;
            particle.position += particle.velocity * delta;
            particle.velocity *= (1.0 - delta * 0.35).max(0.0);
            particle.rotation += particle.angular_velocity * delta;
        }
        if !alive {
            budget.release_smoke(cloud.reserved_count);
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(mut mesh) = meshes.get_mut(&cloud.mesh) {
            update_smoke_mesh(
                &mut mesh,
                &cloud.particles,
                elapsed,
                smoke_right,
                smoke_up,
                smoke_normal,
                config.smoke.max_opacity,
            );
        }
    }
}

fn random_direction(rng: &mut impl Rng) -> Vec3 {
    let direction = Vec3::new(
        rng.random_range(-1.0..1.0),
        rng.random_range(-1.0..1.0),
        rng.random_range(-1.0..1.0),
    ) + Vec3::Y * EXPLOSION_SHARD_UP_BIAS;
    if direction.length_squared() <= f32::EPSILON {
        Vec3::Y
    } else {
        direction.normalize()
    }
}

fn bounce_on_surface(particle: &mut ShardParticle, plane: SurfacePlane) {
    let from_plane = particle.position - plane.point;
    let planar = from_plane - plane.normal * from_plane.dot(plane.normal);
    if planar.length_squared() > plane.radius * plane.radius {
        return;
    }
    let signed_distance = from_plane.dot(plane.normal) - particle.size * 0.5;
    let normal_speed = particle.velocity.dot(plane.normal);
    if signed_distance >= 0.0 || normal_speed >= 0.0 {
        return;
    }
    particle.position -= plane.normal * signed_distance;
    let tangent_velocity = particle.velocity - plane.normal * normal_speed;
    particle.velocity =
        tangent_velocity * EXPLOSION_SHARD_FRICTION - plane.normal * normal_speed * EXPLOSION_SHARD_BOUNCE_DAMPING;
}

fn particle_mesh(particles: &[ShardParticle], elapsed: f32) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    let indices = repeated_indices(particles.len(), CUBE_VERTICES.len(), &CUBE_INDICES);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, shard_positions(particles, elapsed));
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, shard_normals(particles, elapsed));
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, shard_colors(particles, elapsed));
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn update_particle_mesh(mesh: &mut Mesh, particles: &[ShardParticle], elapsed: f32) {
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, shard_positions(particles, elapsed));
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, shard_normals(particles, elapsed));
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, shard_colors(particles, elapsed));
}

fn shard_positions(particles: &[ShardParticle], elapsed: f32) -> Vec<[f32; 3]> {
    let mut positions = Vec::with_capacity(particles.len() * CUBE_VERTICES.len());
    for particle in particles {
        let progress = (elapsed / particle.lifetime).clamp(0.0, 1.0);
        let scale = if shard_is_alive(particle, elapsed) {
            particle.size * (1.0 - progress)
        } else {
            0.0
        };
        for vertex in CUBE_VERTICES {
            positions.push((particle.position + particle.rotation * vertex * scale).to_array());
        }
    }
    positions
}

fn shard_normals(particles: &[ShardParticle], elapsed: f32) -> Vec<[f32; 3]> {
    let mut normals = Vec::with_capacity(particles.len() * CUBE_NORMALS.len());
    for particle in particles {
        let alive = shard_is_alive(particle, elapsed);
        for normal in CUBE_NORMALS {
            normals.push(if alive {
                (particle.rotation * normal).to_array()
            } else {
                Vec3::Y.to_array()
            });
        }
    }
    normals
}

fn shard_colors(particles: &[ShardParticle], elapsed: f32) -> Vec<[f32; 4]> {
    let mut colors = Vec::with_capacity(particles.len() * CUBE_VERTICES.len());
    for particle in particles {
        let color = if shard_is_alive(particle, elapsed) {
            particle.color
        } else {
            [0.0; 4]
        };
        colors.extend([color; CUBE_VERTICES.len()]);
    }
    colors
}

fn shard_is_alive(particle: &ShardParticle, elapsed: f32) -> bool {
    elapsed < particle.lifetime && particle.max_distance.is_none_or(|limit| particle.travelled < limit)
}

fn smoke_mesh(particles: &[SmokeParticle], elapsed: f32, right: Vec3, up: Vec3, normal: Vec3, max_alpha: f32) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, smoke_positions(particles, elapsed, right, up));
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![normal.to_array(); particles.len() * SMOKE_VERTICES_PER_PARTICLE],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, smoke_colors(particles, elapsed, max_alpha));
    mesh.insert_indices(Indices::U32(smoke_indices(particles.len())));
    mesh
}

fn update_smoke_mesh(
    mesh: &mut Mesh,
    particles: &[SmokeParticle],
    elapsed: f32,
    right: Vec3,
    up: Vec3,
    normal: Vec3,
    max_alpha: f32,
) {
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, smoke_positions(particles, elapsed, right, up));
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![normal.to_array(); particles.len() * SMOKE_VERTICES_PER_PARTICLE],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, smoke_colors(particles, elapsed, max_alpha));
}

fn smoke_positions(particles: &[SmokeParticle], elapsed: f32, right: Vec3, up: Vec3) -> Vec<[f32; 3]> {
    let mut positions = Vec::with_capacity(particles.len() * SMOKE_VERTICES_PER_PARTICLE);
    for particle in particles {
        let progress = (elapsed / particle.lifetime).clamp(0.0, 1.0);
        let scale = particle.start_size + (particle.end_size - particle.start_size) * progress.sqrt();
        positions.push(particle.position.to_array());
        for ring_radius in [SMOKE_INNER_RADIUS, 1.0] {
            for segment in 0..SMOKE_RING_SEGMENTS {
                let angle = segment as f32 / SMOKE_RING_SEGMENTS as f32 * std::f32::consts::TAU + particle.rotation;
                let radial = right * (angle.cos() * particle.aspect.x) + up * (angle.sin() * particle.aspect.y);
                positions.push((particle.position + radial * scale * ring_radius).to_array());
            }
        }
    }
    positions
}

fn smoke_colors(particles: &[SmokeParticle], elapsed: f32, max_alpha: f32) -> Vec<[f32; 4]> {
    let mut colors = Vec::with_capacity(particles.len() * SMOKE_VERTICES_PER_PARTICLE);
    for particle in particles {
        let alpha = smoke_alpha(elapsed, particle.lifetime, max_alpha);
        colors.push([particle.color.x, particle.color.y, particle.color.z, alpha * 0.85]);
        colors.extend([[particle.color.x, particle.color.y, particle.color.z, alpha]; SMOKE_RING_SEGMENTS]);
        colors.extend([[particle.color.x, particle.color.y, particle.color.z, 0.0]; SMOKE_RING_SEGMENTS]);
    }
    colors
}

fn smoke_alpha(elapsed: f32, lifetime: f32, max_alpha: f32) -> f32 {
    if elapsed >= lifetime {
        return 0.0;
    }
    let fade_in = smoothstep((elapsed / EXPLOSION_SMOKE_FADE_IN_SECS).clamp(0.0, 1.0));
    let progress = (elapsed / lifetime).clamp(0.0, 1.0);
    let fade_out_progress = ((progress - EXPLOSION_SMOKE_FADE_OUT_START_FRACTION)
        / (1.0 - EXPLOSION_SMOKE_FADE_OUT_START_FRACTION))
        .clamp(0.0, 1.0);
    max_alpha * fade_in * (1.0 - smoothstep(fade_out_progress))
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn smoke_indices(count: usize) -> Vec<u32> {
    let mut indices = Vec::with_capacity(count * SMOKE_RING_SEGMENTS * 9);
    for particle in 0..count as u32 {
        let base = particle * SMOKE_VERTICES_PER_PARTICLE as u32;
        let inner_start = base + 1;
        let outer_start = inner_start + SMOKE_RING_SEGMENTS as u32;
        for segment in 0..SMOKE_RING_SEGMENTS as u32 {
            let next = (segment + 1) % SMOKE_RING_SEGMENTS as u32;
            let inner = inner_start + segment;
            let inner_next = inner_start + next;
            let outer = outer_start + segment;
            let outer_next = outer_start + next;
            indices.extend([base, inner, inner_next]);
            indices.extend([inner, outer, outer_next, inner, outer_next, inner_next]);
        }
    }
    indices
}

fn repeated_indices(count: usize, vertices_per_particle: usize, template: &[u32]) -> Vec<u32> {
    let mut indices = Vec::with_capacity(count * template.len());
    for particle in 0..count as u32 {
        let base = particle * vertices_per_particle as u32;
        indices.extend(template.iter().map(|index| base + index));
    }
    indices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_budget_clamps_and_releases_particles() {
        let mut budget = ExplosionVfxBudget::default();
        assert_eq!(
            budget.reserve_shards(EXPLOSION_SHARD_GLOBAL_MAX_COUNT + 10, EXPLOSION_SHARD_GLOBAL_MAX_COUNT,),
            EXPLOSION_SHARD_GLOBAL_MAX_COUNT
        );
        assert_eq!(budget.reserve_shards(1, EXPLOSION_SHARD_GLOBAL_MAX_COUNT), 0);
        budget.release_shards(20);
        assert_eq!(budget.reserve_shards(30, EXPLOSION_SHARD_GLOBAL_MAX_COUNT), 20);
    }

    #[test]
    fn repeated_indices_allocate_one_cube_per_particle() {
        let indices = repeated_indices(3, CUBE_VERTICES.len(), &CUBE_INDICES);
        assert_eq!(indices.len(), CUBE_INDICES.len() * 3);
        assert_eq!(indices.iter().copied().max(), Some(71));
    }

    #[test]
    fn smoke_indices_allocate_two_radial_bands_per_particle() {
        let indices = smoke_indices(2);
        assert_eq!(indices.len(), SMOKE_RING_SEGMENTS * 9 * 2);
        assert_eq!(indices.iter().copied().max(), Some(49));
    }

    #[test]
    fn smoke_reaches_full_opacity_as_fireball_ends_then_holds() {
        let config = ExplosionVfxConfig::default();
        let lifetime = config.smoke.lifetime_secs;
        let opacity = config.smoke.max_opacity;
        assert_eq!(smoke_alpha(0.0, lifetime, opacity), 0.0);
        assert!(smoke_alpha(0.25, lifetime, opacity) < smoke_alpha(0.5, lifetime, opacity));
        assert_eq!(smoke_alpha(0.5, lifetime, opacity), opacity);
        assert_eq!(smoke_alpha(2.0, lifetime, opacity), opacity);
        assert!(smoke_alpha(3.5, lifetime, opacity) < opacity);
        assert_eq!(smoke_alpha(lifetime, lifetime, opacity), 0.0);
    }
}
