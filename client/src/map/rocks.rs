use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilityRange,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use common::map::{DecorationKind, GroundDecoration, ROCK_VARIANTS, RockClass, RockShape, rock_noise, rock_shape};

use crate::{
    config::{AssetSet, ClientSettings},
    constants::{
        ROCK_BOULDER_LOD_DISTANCES, ROCK_LICHEN_COLOR, ROCK_PEBBLE_FADE, ROCK_STONE_LOD_DISTANCES, ROCK_TEXTURE_SPAN,
        ROCK_TINTS,
    },
};

// Faces meeting at more than this angle keep a hard edge; flatter joins are
// smoothed, so the chipped facets read as breaks in a rounded body.
const CREASE_COS: f32 = 0.77;

pub(super) struct RockAssets {
    near: HashMap<(RockClass, u32), Vec<Handle<Mesh>>>,
    far: HashMap<(RockClass, u32), Mesh>,
    material: Handle<StandardMaterial>,
}

fn class_index(class: RockClass) -> usize {
    match class {
        RockClass::Pebble => 0,
        RockClass::Stone => 1,
        RockClass::Boulder => 2,
    }
}

// Icosphere subdivisions of each detail level, nearest first, and of the
// merged far mesh; pebbles are never far enough away to need one.
fn near_subdivisions(class: RockClass) -> &'static [u32] {
    match class {
        RockClass::Pebble => &[2],
        RockClass::Stone | RockClass::Boulder => &[3, 2, 1],
    }
}

fn far_subdivisions(class: RockClass) -> Option<u32> {
    match class {
        RockClass::Pebble => None,
        RockClass::Stone => Some(0),
        RockClass::Boulder => Some(1),
    }
}

fn lod_range(class: RockClass, lod: usize) -> VisibilityRange {
    let distances = match class {
        RockClass::Pebble => {
            return VisibilityRange {
                start_margin: 0.0..0.0,
                end_margin: ROCK_PEBBLE_FADE[0]..ROCK_PEBBLE_FADE[1],
                use_aabb: false,
            };
        }
        RockClass::Stone => ROCK_STONE_LOD_DISTANCES,
        RockClass::Boulder => ROCK_BOULDER_LOD_DISTANCES,
    };
    VisibilityRange {
        start_margin: if lod == 0 {
            0.0..0.0
        } else {
            distances[lod - 1][0]..distances[lod - 1][1]
        },
        end_margin: distances[lod][0]..distances[lod][1],
        use_aabb: false,
    }
}

impl RockAssets {
    pub(super) fn new(
        server: &AssetServer,
        asset_set: &AssetSet,
        settings: &ClientSettings,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        let definition = asset_set.rock_material_def();
        let material = materials.add(definition.standard_material(
            server,
            settings.rendering.texture_anisotropy,
            settings.rendering.mipmaps,
        ));
        let mut near = HashMap::new();
        let mut far = HashMap::new();
        for class in RockClass::ALL {
            let uv_scale = ROCK_TEXTURE_SPAN[class_index(class)] / definition.tile_size();
            for variant in 0..ROCK_VARIANTS {
                let tint = ROCK_TINTS[variant as usize % ROCK_TINTS.len()];
                let seed = class_index(class) as u32 * 64 + variant;
                let mesh =
                    |subdivisions: u32| rock_mesh(&rock_shape(class, variant, subdivisions), uv_scale, tint, seed);
                near.insert(
                    (class, variant),
                    near_subdivisions(class)
                        .iter()
                        .map(|&subdivisions| meshes.add(mesh(subdivisions)))
                        .collect(),
                );
                if let Some(subdivisions) = far_subdivisions(class) {
                    far.insert((class, variant), mesh(subdivisions));
                }
            }
        }
        Self { near, far, material }
    }

    // One rock as its own entities: detail levels that fade into each other
    // with distance, the nearer ones of the solid classes casting shadows.
    pub(super) fn spawn(&self, commands: &mut Commands, root: Entity, decoration: &GroundDecoration, class: RockClass) {
        let lods = &self.near[&(class, decoration.variant % ROCK_VARIANTS)];
        for (lod, mesh) in lods.iter().enumerate() {
            let mut entity = commands.spawn((
                ChildOf(root),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(self.material.clone()),
                Transform::default(),
                lod_range(class, lod),
            ));
            if class == RockClass::Pebble || lod == 2 {
                entity.insert(NotShadowCaster);
            }
        }
    }

    // Every stone and boulder of a chunk baked into one mesh around `origin`
    // at its coarsest level.
    pub(super) fn far_chunk(&self, decorations: &[GroundDecoration], origin: Vec3) -> Option<Mesh> {
        let mut merged: Option<Mesh> = None;
        for decoration in decorations {
            let DecorationKind::Rock(class) = decoration.kind else {
                continue;
            };
            let Some(mesh) = self.far.get(&(class, decoration.variant % ROCK_VARIANTS)) else {
                continue;
            };
            let mesh = mesh.clone().transformed_by(
                Transform::from_translation(decoration.position - origin)
                    .with_scale(decoration.scale)
                    .with_rotation(decoration.rotation),
            );
            match &mut merged {
                Some(chunk) => chunk
                    .merge(&mesh)
                    .expect("far rock meshes have incompatible vertex layouts"),
                None => merged = Some(mesh),
            }
        }
        merged.map(|mut mesh| {
            mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
            mesh
        })
    }

    pub(super) fn material(&self) -> Handle<StandardMaterial> {
        self.material.clone()
    }
}

fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// Hard creases where facets meet and smooth shading elsewhere, the stone
// texture projected along each face's dominant axis, and a vertex tint that
// darkens toward the buried base with lichen creeping over the top.
pub(super) fn rock_mesh(shape: &RockShape, uv_scale: f32, tint: Color, seed: u32) -> Mesh {
    let corner = |index: u32| shape.vertices[index as usize];
    let face_normals: Vec<Vec3> = shape
        .triangles
        .iter()
        .map(|&[a, b, c]| (corner(b) - corner(a)).cross(corner(c) - corner(a)))
        .collect();
    let mut faces_of_vertex = vec![Vec::new(); shape.vertices.len()];
    for (face, triangle) in shape.triangles.iter().enumerate() {
        for &index in triangle {
            faces_of_vertex[index as usize].push(face);
        }
    }
    let tint = tint.to_linear().to_vec3();
    let lichen = ROCK_LICHEN_COLOR.to_linear().to_vec3();
    let count = shape.triangles.len() * 3;
    let mut positions = Vec::with_capacity(count);
    let mut normals = Vec::with_capacity(count);
    let mut uvs = Vec::with_capacity(count);
    let mut colors = Vec::with_capacity(count);
    for (face, triangle) in shape.triangles.iter().enumerate() {
        let facing = face_normals[face].normalize();
        let axis = facing.abs().max_position();
        for &index in triangle {
            let position = corner(index);
            let mut normal = Vec3::ZERO;
            for &other in &faces_of_vertex[index as usize] {
                if face_normals[other].normalize().dot(facing) > CREASE_COS {
                    normal += face_normals[other];
                }
            }
            let normal = normal.normalize_or(facing);
            let uv = match axis {
                0 => Vec2::new(position.z, position.y),
                1 => Vec2::new(position.x, position.z),
                _ => Vec2::new(position.x, position.y),
            } * uv_scale;
            let shade = 0.7 + 0.3 * smoothstep(-0.6, 0.2, position.y);
            let moss = rock_noise(position * 2.2 + Vec3::splat(seed as f32 * 0.37), seed) * 0.5 + 0.5;
            let moss = smoothstep(0.55, 0.85, moss) * normal.y.max(0.0) * 0.7;
            let color = (tint * shade).lerp(lichen, moss);
            positions.push(position.to_array());
            normals.push(normal.to_array());
            uvs.push(uv.to_array());
            colors.push(color.extend(1.0).to_array());
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32((0..count as u32).collect()));
    mesh.generate_tangents()
        .expect("rock mesh lacks the positions, normals, or UVs tangents need");
    mesh
}

#[cfg(test)]
#[path = "tests/rocks.rs"]
mod tests;
