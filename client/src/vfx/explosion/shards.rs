use super::particles::{ExplosionVfxBudget, SurfacePlane, random_direction, repeated_indices};
use crate::{config::ExplosionVfxConfig, constants::*};
use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use common::physics::CollisionWorld;
use rand::{Rng, RngExt};

pub(super) const CUBE_VERTICES: [Vec3; 24] = [
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
pub(super) const CUBE_NORMALS: [Vec3; 24] = [
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
pub(super) const CUBE_INDICES: [u32; 36] = [
    0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16, 17, 18, 16, 18, 19, 20, 21,
    22, 20, 22, 23,
];

pub(super) struct ShardParticle {
    pub(super) position: Vec3,
    pub(super) velocity: Vec3,
    pub(super) rotation: Quat,
    pub(super) angular_velocity: Vec3,
    pub(super) size: f32,
    pub(super) lifetime: f32,
    pub(super) travelled: f32,
    pub(super) max_distance: Option<f32>,
    pub(super) color: [f32; 4],
}

#[derive(Component)]
pub struct ExplosionShardCloud {
    pub(super) particles: Vec<ShardParticle>,
    pub(super) elapsed: f32,
    pub(super) mesh: Handle<Mesh>,
    pub(super) ground: Option<SurfacePlane>,
    pub(super) reserved_count: usize,
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

pub(super) fn bounce_on_surface(particle: &mut ShardParticle, plane: SurfacePlane) {
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

pub(super) fn update_particle_mesh(mesh: &mut Mesh, particles: &[ShardParticle], elapsed: f32) {
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
