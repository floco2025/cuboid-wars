use super::particles::ExplosionVfxBudget;
use crate::constants::*;
use crate::vfx::cube::smoothstep;
use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use rand::{Rng, RngExt};
const SMOKE_RING_SEGMENTS: usize = 12;
const SMOKE_INNER_RADIUS: f32 = 0.45;
const SMOKE_VERTICES_PER_PARTICLE: usize = 1 + SMOKE_RING_SEGMENTS * 2;

pub(super) struct SmokeParticle {
    pub(super) position: Vec3,
    pub(super) velocity: Vec3,
    pub(super) rotation: f32,
    pub(super) angular_velocity: f32,
    pub(super) aspect: Vec2,
    pub(super) start_size: f32,
    pub(super) end_size: f32,
    pub(super) lifetime: f32,
    pub(super) color: Vec3,
}

#[derive(Component)]
pub struct ExplosionSmokeCloud {
    pub(super) particles: Vec<SmokeParticle>,
    pub(super) elapsed: f32,
    pub(super) mesh: Handle<Mesh>,
    pub(super) reserved_count: usize,
}

pub(super) fn spawn_smoke_cloud(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    center: Vec3,
    reach_radius: f32,
    requested_count: usize,
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
            end_size: EXPLOSION_SMOKE_END_SIZE * rng.random_range(0.75..1.35),
            lifetime: EXPLOSION_SMOKE_LIFETIME_SECS * rng.random_range(0.9..1.15),
            color: Vec3::new(shade * 1.08, shade, shade * 0.9),
        });
    }
    let mesh = meshes.add(smoke_mesh(
        &particles,
        0.0,
        Vec3::X,
        Vec3::Y,
        Vec3::Z,
        EXPLOSION_SMOKE_MAX_OPACITY,
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

pub(super) fn update_smoke_mesh(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_indices_allocate_two_radial_bands_per_particle() {
        let indices = smoke_indices(2);
        assert_eq!(indices.len(), SMOKE_RING_SEGMENTS * 9 * 2);
        assert_eq!(indices.iter().copied().max(), Some(49));
    }

    #[test]
    fn smoke_reaches_full_opacity_as_fireball_ends_then_holds() {
        let lifetime = EXPLOSION_SMOKE_LIFETIME_SECS;
        let opacity = EXPLOSION_SMOKE_MAX_OPACITY;
        assert_eq!(smoke_alpha(0.0, lifetime, opacity), 0.0);
        assert!(smoke_alpha(0.25, lifetime, opacity) < smoke_alpha(0.5, lifetime, opacity));
        assert_eq!(smoke_alpha(0.5, lifetime, opacity), opacity);
        assert_eq!(smoke_alpha(2.0, lifetime, opacity), opacity);
        assert!(smoke_alpha(3.5, lifetime, opacity) < opacity);
        assert_eq!(smoke_alpha(lifetime, lifetime, opacity), 0.0);
    }
}
