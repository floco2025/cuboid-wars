use std::{
    array,
    f32::consts::{FRAC_PI_2, PI, TAU},
};

use bevy::{asset::RenderAssetUsages, prelude::*, render::render_resource::PrimitiveTopology};

use crate::constants::{COOKIE_SIZE, ITEM_COIN_COLOR};

use super::pickup_material;

const FACE_RADIUS: f32 = 0.72;
const FACE_DEPTH: f32 = 0.14;
const SEGMENTS: u32 = 48;

pub struct CoinAssets {
    relief_mesh: Handle<Mesh>,
    face_mesh: Handle<Mesh>,
    relief_material: Handle<StandardMaterial>,
    face_material: Handle<StandardMaterial>,
}

impl CoinAssets {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, glow: f32) -> Self {
        let mut relief = rim_mesh();
        let star = star_mesh();
        let back_star = star.clone().rotated_by(Quat::from_rotation_y(PI));
        for mesh in [star, back_star] {
            relief
                .merge(&mesh)
                .expect("coin relief mesh attributes are incompatible");
        }

        let mut faces = Circle::new(FACE_RADIUS * COOKIE_SIZE)
            .mesh()
            .resolution(SEGMENTS)
            .build()
            .translated_by(Vec3::Z * FACE_DEPTH * COOKIE_SIZE);
        let back_face = faces.clone().rotated_by(Quat::from_rotation_y(PI));
        faces
            .merge(&back_face)
            .expect("coin face mesh attributes are incompatible");

        let mut relief_material = pickup_material(ITEM_COIN_COLOR, glow);
        relief_material.metallic = 0.8;
        relief_material.perceptual_roughness = 0.28;
        let mut face_material = pickup_material(Color::srgb(0.78, 0.48, 0.06), glow);
        face_material.metallic = 0.7;
        face_material.perceptual_roughness = 0.4;

        Self {
            relief_mesh: meshes.add(relief),
            face_mesh: meshes.add(faces),
            relief_material: materials.add(relief_material),
            face_material: materials.add(face_material),
        }
    }
}

pub fn spawn_coin_visual(parent: &mut ChildSpawnerCommands, assets: &CoinAssets) {
    for (mesh, material) in [
        (&assets.relief_mesh, &assets.relief_material),
        (&assets.face_mesh, &assets.face_material),
    ] {
        parent.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::IDENTITY,
        ));
    }
}

fn rim_mesh() -> Mesh {
    let profile = [
        Vec2::new(FACE_RADIUS, -FACE_DEPTH),
        Vec2::new(0.78, -0.22),
        Vec2::new(0.9, -0.22),
        Vec2::new(1.0, -FACE_DEPTH),
        Vec2::new(1.0, FACE_DEPTH),
        Vec2::new(0.9, 0.22),
        Vec2::new(0.78, 0.22),
        Vec2::new(FACE_RADIUS, FACE_DEPTH),
    ];
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for band in profile.windows(2) {
        let [a, b] = [band[0], band[1]];
        let tangent = b - a;
        let normal = Vec2::new(tangent.y, -tangent.x).normalize();
        for segment in 0..SEGMENTS {
            let start = segment as f32 / SEGMENTS as f32;
            let end = (segment + 1) as f32 / SEGMENTS as f32;
            for (point, turn) in [(a, start), (a, end), (b, end), (a, start), (b, end), (b, start)] {
                let (sin, cos) = (turn * TAU).sin_cos();
                positions.push((Vec3::new(point.x * cos, point.x * sin, point.y) * COOKIE_SIZE).to_array());
                normals.push([normal.x * cos, normal.x * sin, normal.y]);
                uvs.push([turn, point.y]);
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
}

fn star_mesh() -> Mesh {
    let points: [Vec2; 10] = array::from_fn(|index| {
        let angle = FRAC_PI_2 + index as f32 * TAU / 10.0;
        let radius = if index % 2 == 0 { 0.58 } else { 0.28 };
        Vec2::new(angle.cos(), angle.sin()) * radius
    });
    let mut positions = Vec::new();
    for (&a, &b) in points.iter().zip(points.iter().cycle().skip(1)) {
        let lower_a = a.extend(FACE_DEPTH);
        let lower_b = b.extend(FACE_DEPTH);
        let upper_a = (a * 0.84).extend(0.27);
        let upper_b = (b * 0.84).extend(0.27);
        for triangle in [
            [lower_a, lower_b, upper_b],
            [lower_a, upper_b, upper_a],
            [Vec3::Z * 0.27, upper_a, upper_b],
        ] {
            positions.extend(triangle.map(|point| (point * COOKIE_SIZE).to_array()));
        }
    }
    let uvs: Vec<[f32; 2]> = positions.iter().map(|point| [point[0], point[1]]).collect();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_computed_flat_normals()
}
