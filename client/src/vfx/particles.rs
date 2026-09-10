use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use super::cube::{CUBE_INDICES, CUBE_NORMALS, CUBE_VERTICES, repeated_indices};

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
    pub exhaust: ParticleCloud,
}

impl ParticleClouds {
    fn iter_mut(&mut self) -> impl Iterator<Item = &mut ParticleCloud> {
        [
            &mut self.drops,
            &mut self.splashes,
            &mut self.sparkles,
            &mut self.sparks,
            &mut self.exhaust,
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
            exhaust: spawn_cloud(world, &material, "missile exhaust"),
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
                mesh.insert_indices(Indices::U32(repeated_indices(
                    capacity,
                    CUBE_VERTICES.len(),
                    &CUBE_INDICES,
                )));
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
    mesh.insert_indices(Indices::U32(repeated_indices(
        max_particles,
        CUBE_VERTICES.len(),
        &CUBE_INDICES,
    )));
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

#[cfg(test)]
#[path = "tests/particles.rs"]
mod tests;
