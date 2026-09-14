use std::f32::consts::TAU;

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilityRange,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use common::map::{DecorationKind, GroundDecoration};
use rand::{RngExt, SeedableRng, rngs::SmallRng};

use crate::{
    constants::{
        GRASS_WIND_DIRECTION_DEGREES, TREE_BARK_COLOR, TREE_FOLIAGE_CUTOFF, TREE_FOLIAGE_TRANSMISSION,
        TREE_LOD_DISTANCES, TREE_VARIANTS, TREE_WIND_SPEED, TREE_WIND_STRENGTH,
    },
    materials::{TreeMaterial, TreeWindExtension},
};

// The three near detail levels, then the far one that whole chunks of
// distant trees merge into.
const NEAR_LODS: usize = 3;
const FAR_LOD: usize = 3;

pub(super) struct TreeAssets {
    near: Vec<Vec<(Handle<Mesh>, Handle<Mesh>)>>,
    far: Vec<(Mesh, Mesh)>,
    bark: Handle<TreeMaterial>,
    foliage: Handle<TreeMaterial>,
    far_bark: Handle<StandardMaterial>,
    far_foliage: Handle<StandardMaterial>,
}

impl TreeAssets {
    pub(super) fn new(
        server: &AssetServer,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        tree_materials: &mut Assets<TreeMaterial>,
    ) -> Self {
        let trees: Vec<Tree> = (0..TREE_VARIANTS).map(Tree::grow).collect();
        let near = trees
            .iter()
            .map(|tree| {
                (0..NEAR_LODS)
                    .map(|lod| {
                        let (wood, leaves) = tree.meshes(lod);
                        (meshes.add(wood), meshes.add(leaves))
                    })
                    .collect()
            })
            .collect();
        let far = trees.iter().map(|tree| tree.meshes(FAR_LOD)).collect();
        let wind_direction = Vec2::from_angle(GRASS_WIND_DIRECTION_DEGREES.to_radians());
        let wind = TreeWindExtension {
            wind: Vec4::new(wind_direction.x, wind_direction.y, TREE_WIND_STRENGTH, TREE_WIND_SPEED),
        };
        Self {
            near,
            far,
            bark: tree_materials.add(TreeMaterial {
                base: bark_material(),
                extension: wind.clone(),
            }),
            foliage: tree_materials.add(TreeMaterial {
                base: foliage_material(server),
                extension: wind,
            }),
            far_bark: materials.add(bark_material()),
            far_foliage: materials.add(foliage_material(server)),
        }
    }

    // One tree as its own entities: three detail levels that fade into each
    // other with distance, swaying in the wind, the near two casting shadows.
    pub(super) fn spawn(&self, commands: &mut Commands, root: Entity, decoration: &GroundDecoration) {
        let variant = &self.near[decoration.variant as usize % self.near.len()];
        for (lod, (wood, leaves)) in variant.iter().enumerate() {
            let range = VisibilityRange {
                start_margin: if lod == 0 {
                    0.0..0.0
                } else {
                    TREE_LOD_DISTANCES[lod - 1][0]..TREE_LOD_DISTANCES[lod - 1][1]
                },
                end_margin: TREE_LOD_DISTANCES[lod][0]..TREE_LOD_DISTANCES[lod][1],
                use_aabb: false,
            };
            for (mesh, material) in [(wood, &self.bark), (leaves, &self.foliage)] {
                let mut entity = commands.spawn((
                    ChildOf(root),
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    range.clone(),
                ));
                if lod == 2 {
                    entity.insert(NotShadowCaster);
                }
            }
        }
    }

    // Every tree of a chunk baked into one wood and one leaf mesh around
    // `origin`, at the far detail level; still, since sway is invisible at
    // that distance and the merged mesh has no per-tree transform.
    pub(super) fn far_chunk(&self, decorations: &[GroundDecoration], origin: Vec3) -> Option<(Mesh, Mesh)> {
        let mut merged: Option<(Mesh, Mesh)> = None;
        for decoration in decorations {
            if decoration.kind != DecorationKind::Tree {
                continue;
            }
            let (wood, leaves) = &self.far[decoration.variant as usize % self.far.len()];
            let transform = Transform::from_translation(decoration.position - origin)
                .with_scale(decoration.scale)
                .with_rotation(decoration.rotation);
            let wood = wood.clone().transformed_by(transform);
            let leaves = leaves.clone().transformed_by(transform);
            match &mut merged {
                Some((chunk_wood, chunk_leaves)) => {
                    chunk_wood
                        .merge(&wood)
                        .expect("far tree wood meshes have incompatible vertex layouts");
                    chunk_leaves
                        .merge(&leaves)
                        .expect("far tree leaf meshes have incompatible vertex layouts");
                }
                None => merged = Some((wood, leaves)),
            }
        }
        merged.map(|(mut wood, mut leaves)| {
            wood.asset_usage = RenderAssetUsages::RENDER_WORLD;
            leaves.asset_usage = RenderAssetUsages::RENDER_WORLD;
            (wood, leaves)
        })
    }

    pub(super) fn far_materials(&self) -> (Handle<StandardMaterial>, Handle<StandardMaterial>) {
        (self.far_bark.clone(), self.far_foliage.clone())
    }
}

fn bark_material() -> StandardMaterial {
    StandardMaterial {
        base_color: TREE_BARK_COLOR,
        perceptual_roughness: 0.95,
        reflectance: 0.15,
        ..default()
    }
}

fn foliage_material(server: &AssetServer) -> StandardMaterial {
    StandardMaterial {
        base_color_texture: Some(server.load("textures/trees/oak-leaf-spray.png")),
        alpha_mode: AlphaMode::Mask(TREE_FOLIAGE_CUTOFF),
        double_sided: true,
        cull_mode: None,
        perceptual_roughness: 0.9,
        reflectance: 0.12,
        diffuse_transmission: TREE_FOLIAGE_TRANSMISSION,
        ..default()
    }
}

struct Limb {
    points: Vec<(Vec3, f32)>,
    detail: bool,
}

struct Shoot {
    tip: Vec3,
    direction: Vec3,
    seed: u64,
}

struct Tree {
    limbs: Vec<Limb>,
    shoots: Vec<Shoot>,
}

impl Tree {
    fn grow(variant: usize) -> Self {
        let mut rng = SmallRng::seed_from_u64(7919 + variant as u64 * 104729);
        let mut tree = Self {
            limbs: Vec::new(),
            shoots: Vec::new(),
        };
        let height = [8.5, 9.5, 7.8][variant % 3];
        let spread = [1.0, 0.83, 1.16][variant % 3];
        let trunk = |t: f32| Vec3::new((t * 2.8).sin() * t * 0.4, t * height, (t * 4.0).sin() * t * 0.3);
        tree.limbs.push(Limb {
            points: (0..=9)
                .map(|i| {
                    let t = i as f32 / 9.0;
                    let radius = 0.32 * (1.0 - t).powf(0.85) + 0.012 + 0.15 * (-t * 30.0).exp();
                    (trunk(t), radius)
                })
                .collect(),
            detail: false,
        });
        for root in 0..6 {
            let angle = root as f32 * TAU / 6.0 + rng.random_range(-0.2..0.2);
            let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
            tree.limbs.push(Limb {
                points: vec![
                    (Vec3::Y * 0.55, 0.17),
                    (radial * 0.45 + Vec3::Y * 0.12, 0.11),
                    (radial * 1.05, 0.01),
                ],
                detail: false,
            });
        }
        for branch in 0..15 {
            let t = 0.34 + branch as f32 * 0.039;
            let angle = branch as f32 * 2.399963 + rng.random_range(-0.35..0.35);
            let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
            let side = Vec3::new(-radial.z, 0.0, radial.x);
            let reach = (3.5 * (1.0 - t).sqrt() + rng.random_range(-0.3..0.45)) * spread;
            let base = trunk(t);
            let elbow = base + radial * reach * 0.5 + Vec3::Y * 0.55;
            let tip = base + radial * reach + Vec3::Y * rng.random_range(0.8..1.6);
            tree.limbs.push(Limb {
                points: vec![(base, 0.14 * (1.0 - t) + 0.055), (elbow, 0.06), (tip, 0.012)],
                detail: false,
            });
            for fork in 0..4 {
                let along = 0.4 + fork as f32 * 0.19;
                let base = elbow.lerp(tip, along);
                let sign = if fork % 2 == 0 { 1.0 } else { -1.0 };
                let end = base
                    + side * sign * rng.random_range(0.55..1.15)
                    + radial * 0.25
                    + Vec3::Y * rng.random_range(0.3..0.85);
                tree.limbs.push(Limb {
                    points: vec![
                        (base, 0.035),
                        (base.lerp(end, 0.55) - Vec3::Y * 0.12, 0.018),
                        (end, 0.003),
                    ],
                    detail: true,
                });
                tree.shoots.push(Shoot {
                    tip: end,
                    direction: (end - base).normalize(),
                    seed: rng.random(),
                });
            }
        }
        tree
    }

    // Levels 0-2 thin the limbs and sprays; level 3 keeps the trunk and a
    // few big sprays, enough for a tree that is a dozen pixels tall.
    fn meshes(&self, lod: usize) -> (Mesh, Mesh) {
        let mut wood = TreeMesh::default();
        for (index, limb) in self.limbs.iter().enumerate() {
            let kept = match lod {
                0 | 1 => true,
                2 => !limb.detail,
                _ => index == 0,
            };
            if kept {
                wood.limb(&limb.points, [10, 7, 5, 4][lod]);
            }
        }
        let mut leaves = TreeMesh::default();
        for (index, shoot) in self.shoots.iter().enumerate() {
            if lod == FAR_LOD && index % 4 != 0 {
                continue;
            }
            let mut rng = SmallRng::seed_from_u64(shoot.seed);
            for _ in 0..[9, 5, 3, 1][lod] {
                let offset = Vec3::new(
                    rng.random_range(-0.65..0.65),
                    rng.random_range(-0.4..0.65),
                    rng.random_range(-0.65..0.65),
                );
                let center = shoot.tip + offset;
                let direction = (shoot.direction * 0.35 + offset + Vec3::Y * 0.6).normalize();
                let rotation =
                    Quat::from_rotation_arc(Vec3::Y, direction) * Quat::from_rotation_y(rng.random_range(0.0..TAU));
                let size = rng.random_range(0.85..1.25) * [1.0, 1.3, 1.62, 3.2][lod];
                let brightness = rng.random_range(0.68..1.0) * (0.75 + (center.y / 10.0).clamp(0.0, 1.0) * 0.25);
                leaves.spray(center, rotation, size, brightness);
            }
        }
        (wood.finish(), leaves.finish())
    }
}

#[derive(Default)]
struct TreeMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl TreeMesh {
    fn limb(&mut self, points: &[(Vec3, f32)], sides: usize) {
        let start = self.positions.len() as u32;
        for (ring, &(point, radius)) in points.iter().enumerate() {
            let direction = (points[(ring + 1).min(points.len() - 1)].0 - points[ring.saturating_sub(1)].0).normalize();
            let rotation = Quat::from_rotation_arc(Vec3::Y, direction);
            for side in 0..sides {
                let angle = side as f32 * TAU / sides as f32;
                let normal = rotation * Vec3::new(angle.cos(), 0.0, angle.sin());
                let ridge = 1.0 + (angle * 5.0 + point.y * 0.4).sin() * 0.09;
                let brightness = 0.7 + (angle * 5.0 + point.y * 0.7).cos() * 0.2;
                self.positions.push((point + normal * radius * ridge).to_array());
                self.normals.push(normal.to_array());
                self.colors
                    .push([brightness, brightness * 0.97, brightness * 0.91, 1.0]);
                self.uvs.push([side as f32 / sides as f32, point.y]);
                if ring > 0 {
                    let a = start + ((ring - 1) * sides + side) as u32;
                    let b = start + ((ring - 1) * sides + (side + 1) % sides) as u32;
                    let c = a + sides as u32;
                    let d = b + sides as u32;
                    self.indices.extend([a, c, b, b, c, d]);
                }
            }
        }
    }

    fn spray(&mut self, center: Vec3, rotation: Quat, size: f32, brightness: f32) {
        let start = self.positions.len() as u32;
        for (position, uv) in [
            (Vec3::new(-0.5, -0.5, 0.0), [0.0, 1.0]),
            (Vec3::new(0.0, -0.5, 0.12), [0.5, 1.0]),
            (Vec3::new(0.5, -0.5, 0.0), [1.0, 1.0]),
            (Vec3::new(-0.5, 0.5, 0.0), [0.0, 0.0]),
            (Vec3::new(0.0, 0.5, 0.12), [0.5, 0.0]),
            (Vec3::new(0.5, 0.5, 0.0), [1.0, 0.0]),
        ] {
            let point = center + rotation * position * size;
            let normal =
                (Vec3::new(point.x, (point.y - 4.0).max(0.5), point.z).normalize() + Vec3::Y * 0.5).normalize();
            self.positions.push(point.to_array());
            self.normals.push(normal.to_array());
            self.colors.push([brightness, brightness, brightness * 0.92, 1.0]);
            self.uvs.push(uv);
        }
        self.indices
            .extend([0, 1, 3, 1, 4, 3, 1, 2, 4, 2, 5, 4].map(|i| start + i));
    }

    fn finish(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
            .with_inserted_indices(Indices::U32(self.indices))
    }
}

#[cfg(test)]
#[path = "tests/trees.rs"]
mod tests;
