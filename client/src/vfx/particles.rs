use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::config::ClientSettings;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ParticlePriority {
    Ambient,
    Impact,
    Cue,
}

pub(super) struct ParticleSpawn {
    pub position: Vec3,
    pub velocity: Vec3,
    pub acceleration: Vec3,
    pub start_size: f32,
    pub end_size: f32,
    // Per-axis multiplier on the size — `Vec3::ONE` is a cube; rain streaks
    // stretch the Y axis into a thin vertical line.
    pub stretch: Vec3,
    // The pool is opaque, so the standard end-of-life "fade" darkens toward
    // BLACK — right for hot sparks dying out, wrong for things that stay lit
    // until they vanish (rain drops against a bright sky turn into black
    // bars). `false` keeps full brightness for the whole lifetime.
    pub fades: bool,
    pub lifetime: f32,
    pub color: Vec3,
    pub priority: ParticlePriority,
}

struct TransientParticle {
    position: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
    start_size: f32,
    end_size: f32,
    stretch: Vec3,
    fades: bool,
    lifetime: f32,
    elapsed: f32,
    color: Vec3,
    priority: ParticlePriority,
}

#[derive(Resource)]
pub struct TransientParticles {
    particles: Vec<TransientParticle>,
    mesh: Handle<Mesh>,
    max_particles: usize,
}

impl FromWorld for TransientParticles {
    fn from_world(world: &mut World) -> Self {
        let max_particles = world.resource::<ClientSettings>().vfx.max_transient_particles;
        let mesh = world.resource_mut::<Assets<Mesh>>().add(particle_mesh(max_particles));
        let material = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            ..default()
        });
        world.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            NotShadowCaster,
            NotShadowReceiver,
            NoFrustumCulling,
            Transform::default(),
        ));

        Self {
            particles: Vec::with_capacity(max_particles),
            mesh,
            max_particles,
        }
    }
}

impl TransientParticles {
    pub(super) fn spawn(&mut self, particle: ParticleSpawn) -> bool {
        if self.particles.len() >= self.max_particles {
            let Some(index) = self
                .particles
                .iter()
                .position(|existing| existing.priority < particle.priority)
            else {
                return false;
            };
            self.particles.swap_remove(index);
        }
        self.particles.push(TransientParticle {
            position: particle.position,
            velocity: particle.velocity,
            acceleration: particle.acceleration,
            start_size: particle.start_size,
            end_size: particle.end_size,
            stretch: particle.stretch,
            fades: particle.fades,
            lifetime: particle.lifetime,
            elapsed: 0.0,
            color: particle.color,
            priority: particle.priority,
        });
        true
    }
}

pub fn transient_particles_system(
    time: Res<Time>,
    mut particles: ResMut<TransientParticles>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    advance_particles(&mut particles.particles, time.delta_secs());
    if let Some(mut mesh) = meshes.get_mut(&particles.mesh) {
        update_particle_mesh(&mut mesh, &particles.particles, particles.max_particles);
    }
}

fn advance_particles(particles: &mut Vec<TransientParticle>, delta: f32) {
    for particle in particles.iter_mut() {
        particle.elapsed += delta;
        particle.velocity += particle.acceleration * delta;
        particle.position += particle.velocity * delta;
    }
    particles.retain(|particle| particle.elapsed < particle.lifetime);
}

fn particle_mesh(max_particles: usize) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    let vertex_count = max_particles * CUBE_VERTICES.len();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; vertex_count]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![Vec3::Y.to_array(); vertex_count]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0; 4]; vertex_count]);
    mesh.insert_indices(Indices::U32(repeated_indices(max_particles)));
    mesh
}

fn update_particle_mesh(mesh: &mut Mesh, particles: &[TransientParticle], max_particles: usize) {
    let vertex_count = max_particles * CUBE_VERTICES.len();
    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut colors = Vec::with_capacity(vertex_count);

    for particle in particles {
        let progress = (particle.elapsed / particle.lifetime).clamp(0.0, 1.0);
        let size = particle.start_size + (particle.end_size - particle.start_size) * progress;
        let brightness = if particle.fades {
            (1.0 - progress * progress).max(0.0)
        } else {
            1.0
        };
        let color = (particle.color * brightness).extend(1.0).to_array();
        positions.extend(
            CUBE_VERTICES
                .iter()
                .map(|vertex| (particle.position + *vertex * size * particle.stretch).to_array()),
        );
        normals.extend(CUBE_NORMALS.iter().map(|normal| normal.to_array()));
        colors.extend([color; CUBE_VERTICES.len()]);
    }

    positions.resize(vertex_count, [0.0; 3]);
    normals.resize(vertex_count, Vec3::Y.to_array());
    colors.resize(vertex_count, [0.0; 4]);

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
}

fn repeated_indices(count: usize) -> Vec<u32> {
    let mut indices = Vec::with_capacity(count * CUBE_INDICES.len());
    for particle in 0..count as u32 {
        let base = particle * CUBE_VERTICES.len() as u32;
        indices.extend(CUBE_INDICES.iter().map(|index| base + index));
    }
    indices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud(max_particles: usize) -> TransientParticles {
        TransientParticles {
            particles: Vec::new(),
            mesh: Handle::default(),
            max_particles,
        }
    }

    fn particle(priority: ParticlePriority) -> ParticleSpawn {
        ParticleSpawn {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            start_size: 1.0,
            end_size: 0.0,
            stretch: Vec3::ONE,
            fades: true,
            lifetime: 1.0,
            color: Vec3::ONE,
            priority,
        }
    }

    #[test]
    fn higher_priority_particles_evict_ambient_particles_at_capacity() {
        let mut cloud = cloud(2);
        assert!(cloud.spawn(particle(ParticlePriority::Ambient)));
        assert!(cloud.spawn(particle(ParticlePriority::Ambient)));
        assert!(cloud.spawn(particle(ParticlePriority::Impact)));
        assert!(cloud.spawn(particle(ParticlePriority::Cue)));

        assert_eq!(cloud.particles.len(), 2);
        assert!(
            cloud
                .particles
                .iter()
                .any(|particle| particle.priority == ParticlePriority::Impact)
        );
        assert!(
            cloud
                .particles
                .iter()
                .any(|particle| particle.priority == ParticlePriority::Cue)
        );
    }

    #[test]
    fn ambient_particles_are_dropped_at_capacity() {
        let mut cloud = cloud(1);
        assert!(cloud.spawn(particle(ParticlePriority::Ambient)));
        assert!(!cloud.spawn(particle(ParticlePriority::Ambient)));
        assert_eq!(cloud.particles.len(), 1);
    }

    #[test]
    fn expired_particles_are_removed() {
        let mut particles = vec![TransientParticle {
            position: Vec3::ZERO,
            velocity: Vec3::X,
            acceleration: Vec3::ZERO,
            start_size: 1.0,
            end_size: 0.0,
            stretch: Vec3::ONE,
            fades: true,
            lifetime: 0.5,
            elapsed: 0.0,
            color: Vec3::ONE,
            priority: ParticlePriority::Ambient,
        }];

        advance_particles(&mut particles, 0.5);
        assert!(particles.is_empty());
    }

    #[test]
    fn particle_mesh_keeps_fixed_capacity() {
        let mesh = particle_mesh(2);

        assert_eq!(mesh.count_vertices(), 2 * CUBE_VERTICES.len());
        assert_eq!(mesh.indices().map(Indices::len), Some(2 * CUBE_INDICES.len()));
    }
}
