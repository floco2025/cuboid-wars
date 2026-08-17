use super::particles::{ExplosionVfxBudget, SurfacePlane, random_direction};
use crate::constants::*;
use crate::vfx::cube::{CUBE_INDICES, CUBE_NORMALS, CUBE_VERTICES, repeated_indices};
use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use common::physics::CollisionWorld;
use rand::{Rng, RngExt};
use std::f32::consts::TAU;

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
    rng: &mut impl Rng,
) {
    let count = budget.reserve_shards(requested_count, EXPLOSION_SHARD_GLOBAL_MAX_COUNT);
    if count == 0 {
        return;
    }
    let base_lifetime = EXPLOSION_BASE_DURATION_SECS * EXPLOSION_SHARD_LIFETIME_FACTOR;
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
                rng.random_range(0.0..TAU),
                rng.random_range(0.0..TAU),
                rng.random_range(0.0..TAU),
            ),
            angular_velocity: Vec3::new(
                rng.random_range(-8.0..8.0),
                rng.random_range(-8.0..8.0),
                rng.random_range(-8.0..8.0),
            ),
            size: EXPLOSION_SHARD_SIZE,
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
