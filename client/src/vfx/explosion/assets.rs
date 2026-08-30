use super::scorch::scorch_mesh;
use crate::constants::*;
use bevy::{asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology};
use common::physics::CollisionWorld;
use std::{collections::HashMap, f32::consts::TAU};

const SHOCKWAVE_RESOLUTION: u32 = 64;

// Blast radii from `SInit` (per actor kind, the player death blast, the
// missile blast). Starts empty (initialized at app build) and is replaced
// when `SInit` arrives; death cues can't reach a handler earlier —
// `network/routing.rs` routes only `SInit` and quest state until then.
#[derive(Resource, Default)]
pub struct BlastRadii {
    pub actors: HashMap<String, f32>,
    pub player: f32,
    pub missile: f32,
}

#[must_use]
pub fn explosion_sound_speed(radius: f32) -> f32 {
    (1.08 - radius * 0.012).clamp(0.84, 1.04)
}

// Shared meshes plus material templates cloned for animated instances.
#[derive(Resource)]
pub struct ExplosionAssets {
    pub(super) fireball_mesh: Handle<Mesh>,
    pub(super) scorch_meshes: Vec<Handle<Mesh>>,
    pub(super) shard_material: Handle<StandardMaterial>,
    pub(super) smoke_material: Handle<StandardMaterial>,
    pub(super) fireball_template: StandardMaterial,
    pub(super) ring_template: StandardMaterial,
    pub(super) scorch_template: StandardMaterial,
}

impl ExplosionAssets {
    // Public (rather than folded into `FromWorld`) so tests can build the
    // resource against plain `Assets` collections.
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let flash = EXPLOSION_FIREBALL_EMISSIVE;
        let ring = EXPLOSION_SHOCKWAVE_EMISSIVE;
        let shard = EXPLOSION_SHARD_EMISSIVE;
        Self {
            // Unit-diameter meshes: `Transform::scale` equals the layer's
            // world diameter in meters.
            fireball_mesh: meshes.add(with_white_vertex_colors(Mesh::from(Sphere::new(0.5)))),
            scorch_meshes: (0..EXPLOSION_SCORCH_MESH_VARIANT_COUNT)
                .map(|variant| meshes.add(scorch_mesh(variant as u64)))
                .collect(),
            shard_material: materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.6, 0.25),
                emissive: LinearRgba::rgb(shard, shard * 0.45, shard * 0.12),
                ..default()
            }),
            smoke_material: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                cull_mode: None,
                ..default()
            }),
            fireball_template: StandardMaterial {
                base_color: Color::srgba(1.0, 0.85, 0.6, EXPLOSION_FIREBALL_START_ALPHA),
                emissive: LinearRgba::rgb(flash, flash * 0.45, flash * 0.12),
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            ring_template: StandardMaterial {
                base_color: Color::srgba(1.0, 0.6, 0.3, EXPLOSION_SHOCKWAVE_START_ALPHA),
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

// Uniform white vertex colors: flips the mesh onto the vertex-color
// pipeline permutation. In this app the plain-mesh Blend permutation
// renders translucent materials wrong (invisible or unlit-white); the
// vertex-color path — which the scorch meshes use — renders correctly.
// White multiplies to identity, so visuals are otherwise unchanged.
pub(crate) fn with_white_vertex_colors(mut mesh: Mesh) -> Mesh {
    let count = mesh.count_vertices();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0, 1.0, 1.0, 1.0]; count]);
    mesh
}

pub(super) fn shockwave_mesh(
    collision_world: Option<&CollisionWorld>,
    center: Vec3,
    surface_normal: Vec3,
    reach_radius: f32,
) -> Mesh {
    let rotation = Quat::from_rotation_arc(Vec3::Y, surface_normal);
    let mut positions = Vec::with_capacity(SHOCKWAVE_RESOLUTION as usize * 2);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut colors = Vec::with_capacity(positions.capacity());
    let mut clear = Vec::with_capacity(SHOCKWAVE_RESOLUTION as usize);

    for segment in 0..SHOCKWAVE_RESOLUTION {
        let angle = segment as f32 / SHOCKWAVE_RESOLUTION as f32 * TAU;
        let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
        positions.push((radial * 0.5).to_array());
        positions.push((radial * (0.5 * (1.0 - EXPLOSION_SHOCKWAVE_THICKNESS_RATIO))).to_array());
        normals.extend([Vec3::Y.to_array(); 2]);
        colors.extend([[1.0, 1.0, 1.0, 1.0]; 2]);

        let world_direction = rotation * radial;
        let horizontal = Vec3::new(world_direction.x, 0.0, world_direction.z).normalize_or_zero();
        clear.push(
            horizontal == Vec3::ZERO
                || collision_world
                    .is_none_or(|world| world.wall_surface_along_ray(center, horizontal, reach_radius).is_none()),
        );
    }

    let mut indices = Vec::with_capacity(SHOCKWAVE_RESOLUTION as usize * 6);
    for segment in 0..SHOCKWAVE_RESOLUTION as usize {
        let next = (segment + 1) % SHOCKWAVE_RESOLUTION as usize;
        if !clear[segment] || !clear[next] {
            continue;
        }
        let outer = (segment * 2) as u32;
        let inner = outer + 1;
        let next_outer = (next * 2) as u32;
        let next_inner = next_outer + 1;
        indices.extend([outer, inner, next_outer, next_outer, inner, next_inner]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

impl FromWorld for ExplosionAssets {
    fn from_world(world: &mut World) -> Self {
        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            Self::new(&mut meshes, &mut materials)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn larger_explosions_have_a_lower_sound_pitch() {
        assert!(explosion_sound_speed(6.0) > explosion_sound_speed(15.0));
        assert_eq!(explosion_sound_speed(100.0), 0.84);
    }
}
