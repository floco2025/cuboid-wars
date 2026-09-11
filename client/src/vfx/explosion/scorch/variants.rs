use std::f32::consts::TAU;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use rand::{Rng, RngExt, SeedableRng, rngs::SmallRng};

use crate::constants::{EXPLOSION_SCORCH_RING_ALPHA, EXPLOSION_SCORCH_RING_RADII};

const SCORCH_RESOLUTION: usize = 128;
const OUTLINE_CONTROL_POINTS: usize = 24;
const DETAIL_CONTROL_POINTS: usize = 17;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScorchOutline {
    radii: [f32; OUTLINE_CONTROL_POINTS],
}

impl ScorchOutline {
    fn random(rng: &mut impl Rng) -> Self {
        Self {
            radii: std::array::from_fn(|_| rng.random_range(0.72..1.0)),
        }
    }

    pub(crate) fn for_mesh(mesh_index: usize) -> Self {
        let seed = u64::try_from(mesh_index).expect("scorch mesh index exceeds u64");
        let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(0x5C0C_4A11));
        Self::random(&mut rng)
    }

    pub(crate) fn radius_factor(self, local_angle: f32) -> f32 {
        smooth_cyclic_sample(&self.radii, local_angle.rem_euclid(TAU) / TAU)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ScorchStyle {
    pub(super) mesh_index: usize,
    rotation: f32,
}

impl ScorchStyle {
    pub(crate) fn random(mesh_count: usize, rng: &mut impl Rng) -> Self {
        Self {
            mesh_index: rng.random_range(0..mesh_count),
            rotation: rng.random_range(0.0..TAU),
        }
    }

    pub(super) fn rotation(self) -> f32 {
        self.rotation
    }
}

// One vertex of a mark in its own plane: a unit disc across x/z, the mark's
// scale being its diameter, carrying the ring colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ScorchVertex {
    pub(super) position: Vec2,
    pub(super) color: [f32; 4],
}

impl ScorchVertex {
    pub(super) fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            position: self.position.lerp(other.position, t),
            color: std::array::from_fn(|i| self.color[i] + (other.color[i] - self.color[i]) * t),
        }
    }
}

// A mark's geometry, before or after it is cut to its surface.
#[derive(Clone, Debug, Default)]
pub(crate) struct ScorchVariant {
    pub(super) vertices: Vec<ScorchVertex>,
    pub(super) triangles: Vec<[u32; 3]>,
}

impl ScorchVariant {
    pub(super) fn mesh(&self) -> Mesh {
        let positions: Vec<[f32; 3]> = self
            .vertices
            .iter()
            .map(|vertex| [vertex.position.x, 0.0, vertex.position.y])
            .collect();
        let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
        let colors: Vec<[f32; 4]> = self.vertices.iter().map(|vertex| vertex.color).collect();
        let indices: Vec<u32> = self.triangles.iter().flatten().copied().collect();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }
}

pub(crate) fn scorch_variant(seed: u64) -> ScorchVariant {
    let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(0x5C0C_4A11));
    let outline = ScorchOutline::random(&mut rng);
    let ring_detail: Vec<Vec<f32>> = (0..EXPLOSION_SCORCH_RING_RADII.len())
        .map(|_| {
            (0..DETAIL_CONTROL_POINTS)
                .map(|_| rng.random_range(-0.04..0.04))
                .collect()
        })
        .collect();
    let alpha_detail: Vec<Vec<f32>> = (0..EXPLOSION_SCORCH_RING_RADII.len() - 1)
        .map(|_| {
            (0..DETAIL_CONTROL_POINTS)
                .map(|_| rng.random_range(-0.10..0.10))
                .collect()
        })
        .collect();

    let mut vertices = Vec::with_capacity(1 + EXPLOSION_SCORCH_RING_RADII.len() * SCORCH_RESOLUTION);
    let mut triangles = Vec::with_capacity(SCORCH_RESOLUTION * (1 + 2 * (EXPLOSION_SCORCH_RING_RADII.len() - 1)));

    vertices.push(ScorchVertex {
        position: Vec2::ZERO,
        color: scorch_color(0.88, 0.0),
    });

    for (ring_index, (&radius, &base_alpha)) in EXPLOSION_SCORCH_RING_RADII
        .iter()
        .zip(&EXPLOSION_SCORCH_RING_ALPHA)
        .enumerate()
    {
        for segment in 0..SCORCH_RESOLUTION {
            let progress = segment as f32 / SCORCH_RESOLUTION as f32;
            let angle = progress * TAU;
            let noise = smooth_cyclic_sample(&outline.radii, progress)
                + smooth_cyclic_sample(&ring_detail[ring_index], progress);
            let max_noise = if ring_index + 1 == EXPLOSION_SCORCH_RING_RADII.len() {
                1.0
            } else {
                1.04
            };
            let ring_radius = radius * noise.clamp(0.55, max_noise);
            let alpha_noise = if ring_index + 1 == EXPLOSION_SCORCH_RING_RADII.len() {
                0.0
            } else {
                smooth_cyclic_sample(&alpha_detail[ring_index], progress)
            };
            vertices.push(ScorchVertex {
                position: Vec2::new(ring_radius * angle.cos(), ring_radius * angle.sin()),
                color: scorch_color((base_alpha + alpha_noise).clamp(0.0, 1.0), ring_index as f32),
            });
        }
    }

    for segment in 0..SCORCH_RESOLUTION {
        let current = 1 + segment as u32;
        let next = 1 + ((segment + 1) % SCORCH_RESOLUTION) as u32;
        triangles.push([0, next, current]);
    }
    for ring_index in 0..EXPLOSION_SCORCH_RING_RADII.len() - 1 {
        let inner_start = 1 + ring_index * SCORCH_RESOLUTION;
        let outer_start = inner_start + SCORCH_RESOLUTION;
        for segment in 0..SCORCH_RESOLUTION {
            let next = (segment + 1) % SCORCH_RESOLUTION;
            let inner = (inner_start + segment) as u32;
            let inner_next = (inner_start + next) as u32;
            let outer = (outer_start + segment) as u32;
            let outer_next = (outer_start + next) as u32;
            triangles.push([inner, outer_next, outer]);
            triangles.push([inner, inner_next, outer_next]);
        }
    }

    ScorchVariant { vertices, triangles }
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
