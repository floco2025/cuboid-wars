use bevy::prelude::*;

// The one cube every particle in the game is drawn as (24 verts, 6 faces),
// shared by the particle clouds and the explosion shard/smoke meshes.
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

// Index buffer for `count` particles of `vertices_per_particle` verts each,
// repeating `template` with a per-particle base offset.
pub(super) fn repeated_indices(count: usize, vertices_per_particle: usize, template: &[u32]) -> Vec<u32> {
    let mut indices = Vec::with_capacity(count * template.len());
    for particle in 0..count as u32 {
        let base = particle * vertices_per_particle as u32;
        indices.extend(template.iter().map(|index| base + index));
    }
    indices
}

// The classic ease-in-out ramp on [0, 1].
pub(super) fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_indices_allocate_one_cube_per_particle() {
        let indices = repeated_indices(3, CUBE_VERTICES.len(), &CUBE_INDICES);
        assert_eq!(indices.len(), CUBE_INDICES.len() * 3);
        assert_eq!(indices.iter().copied().max(), Some(71));
    }

    #[test]
    fn smoothstep_hits_endpoints_and_midpoint() {
        assert_eq!(smoothstep(0.0), 0.0);
        assert_eq!(smoothstep(1.0), 1.0);
        assert_eq!(smoothstep(0.5), 0.5);
    }
}
