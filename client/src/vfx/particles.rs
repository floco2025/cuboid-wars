use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

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

// Per-frame cost (attribute rebuild, GPU upload, vertex work) scales with a
// cloud's mesh capacity, so capacity must track the effect's RECENT load, not
// its all-time peak: it grows the moment the live count passes it and shrinks
// once a spike has aged out of both peak windows. The floor keeps idle clouds
// from thrashing through tiny power-of-two steps.
const MIN_CAPACITY: usize = 64;
const SHRINK_WINDOW_SECS: f32 = 3.0;

pub(super) struct ParticleSpawn {
    pub position: Vec3,
    pub velocity: Vec3,
    pub acceleration: Vec3,
    pub start_size: f32,
    pub end_size: f32,
    // Per-axis multiplier on the size — `Vec3::ONE` is a cube; rain streaks
    // stretch the Y axis into a thin vertical line.
    pub stretch: Vec3,
    // The mesh is opaque, so the standard end-of-life "fade" darkens toward
    // BLACK — right for hot sparks dying out, wrong for things that stay lit
    // until they vanish (rain drops against a bright sky turn into black
    // bars). `false` keeps full brightness for the whole lifetime.
    pub fades: bool,
    pub lifetime: f32,
    pub color: Vec3,
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
}

// One effect's short-lived particles, batched into a single vertex-colored
// mesh (one draw call; per-particle color without per-entity materials).
pub struct ParticleCloud {
    particles: Vec<TransientParticle>,
    mesh: Handle<Mesh>,
    capacity: usize,
    label: &'static str,
    // Rolling peak of the live count over the current and previous window —
    // the capacity target never drops below either, so a spike holds its
    // capacity for one to two windows and then releases it.
    window_peak: usize,
    previous_window_peak: usize,
    window_elapsed: f32,
}

impl ParticleCloud {
    fn new(label: &'static str, mesh: Handle<Mesh>) -> Self {
        Self {
            particles: Vec::new(),
            mesh,
            capacity: MIN_CAPACITY,
            label,
            window_peak: 0,
            previous_window_peak: 0,
            window_elapsed: 0.0,
        }
    }

    pub(super) fn spawn(&mut self, particle: ParticleSpawn) {
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
        });
    }

    // Moves and expires particles, then retargets the mesh capacity to the
    // recent peak. Returns the new capacity when it changed so the caller
    // can resize the mesh indices.
    fn advance(&mut self, delta: f32) -> Option<usize> {
        for particle in &mut self.particles {
            particle.elapsed += delta;
            particle.velocity += particle.acceleration * delta;
            particle.position += particle.velocity * delta;
        }
        self.particles.retain(|particle| particle.elapsed < particle.lifetime);

        let live = self.particles.len();
        self.window_peak = self.window_peak.max(live);
        self.window_elapsed += delta;
        if self.window_elapsed >= SHRINK_WINDOW_SECS {
            self.previous_window_peak = self.window_peak;
            self.window_peak = live;
            self.window_elapsed = 0.0;
        }

        let target = live
            .max(self.window_peak)
            .max(self.previous_window_peak)
            .next_power_of_two()
            .max(MIN_CAPACITY);
        if target == self.capacity {
            return None;
        }
        let verb = if target > self.capacity { "grew" } else { "shrank" };
        debug!("{} particle cloud {verb} to {target} slots ({live} live)", self.label);
        self.capacity = target;
        Some(target)
    }
}

#[derive(Resource)]
pub struct ParticleClouds {
    pub drops: ParticleCloud,
    pub splashes: ParticleCloud,
    pub sparkles: ParticleCloud,
    pub sparks: ParticleCloud,
}

impl ParticleClouds {
    fn iter_mut(&mut self) -> impl Iterator<Item = &mut ParticleCloud> {
        [
            &mut self.drops,
            &mut self.splashes,
            &mut self.sparkles,
            &mut self.sparks,
        ]
        .into_iter()
    }
}

impl FromWorld for ParticleClouds {
    fn from_world(world: &mut World) -> Self {
        let material = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            ..default()
        });
        Self {
            drops: spawn_cloud(world, &material, "rain drops"),
            splashes: spawn_cloud(world, &material, "rain splashes"),
            sparkles: spawn_cloud(world, &material, "beam-in sparkles"),
            sparks: spawn_cloud(world, &material, "impact sparks"),
        }
    }
}

fn spawn_cloud(world: &mut World, material: &Handle<StandardMaterial>, label: &'static str) -> ParticleCloud {
    let mesh = world.resource_mut::<Assets<Mesh>>().add(particle_mesh(MIN_CAPACITY));
    world.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        NotShadowCaster,
        NotShadowReceiver,
        NoFrustumCulling,
        Transform::default(),
    ));
    ParticleCloud::new(label, mesh)
}

pub fn particle_clouds_system(time: Res<Time>, mut clouds: ResMut<ParticleClouds>, mut meshes: ResMut<Assets<Mesh>>) {
    let delta = time.delta_secs();
    for cloud in clouds.iter_mut() {
        let resized = cloud.advance(delta);
        if let Some(mut mesh) = meshes.get_mut(&cloud.mesh) {
            if let Some(capacity) = resized {
                mesh.insert_indices(Indices::U32(repeated_indices(capacity)));
            }
            update_particle_mesh(&mut mesh, &cloud.particles, cloud.capacity);
        }
    }
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

    fn cloud() -> ParticleCloud {
        ParticleCloud::new("test", Handle::default())
    }

    fn particle() -> ParticleSpawn {
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
        }
    }

    fn spike(cloud: &mut ParticleCloud, count: usize) {
        for _ in 0..count {
            cloud.spawn(particle());
        }
    }

    #[test]
    fn cloud_grows_immediately_on_spike() {
        let mut cloud = cloud();
        spike(&mut cloud, 100);

        assert_eq!(cloud.particles.len(), 100, "spawns past capacity must not be dropped");
        assert_eq!(cloud.advance(0.0), Some(128), "100 live grows to the next power of two");
        assert_eq!(cloud.advance(0.0), None, "no repeat resize while the load is steady");
    }

    #[test]
    fn recent_peak_holds_capacity() {
        let mut cloud = cloud();
        spike(&mut cloud, 100);
        cloud.advance(0.0);

        // All particles expire, but the spike is still inside the peak
        // window — capacity must not drop yet.
        assert_eq!(cloud.advance(1.0), None);
        assert!(cloud.particles.is_empty());
        assert_eq!(cloud.capacity, 128);
    }

    #[test]
    fn cloud_shrinks_after_spike_leaves_the_window() {
        let mut cloud = cloud();
        spike(&mut cloud, 100);
        cloud.advance(0.0);
        cloud.advance(1.0);

        assert_eq!(
            cloud.advance(SHRINK_WINDOW_SECS),
            None,
            "spike still in the previous window"
        );
        assert_eq!(
            cloud.advance(SHRINK_WINDOW_SECS),
            Some(MIN_CAPACITY),
            "both windows past the spike release the capacity"
        );
    }

    #[test]
    fn particle_mesh_keeps_fixed_capacity() {
        let mesh = particle_mesh(2);

        assert_eq!(mesh.count_vertices(), 2 * CUBE_VERTICES.len());
        assert_eq!(mesh.indices().map(Indices::len), Some(2 * CUBE_INDICES.len()));
    }
}
