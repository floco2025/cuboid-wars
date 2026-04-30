use std::collections::BTreeMap;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{MeshVertexAttribute, VertexAttributeValues},
    prelude::*,
    render::render_resource::PrimitiveTopology,
};
use rand::{RngExt, rng};

use crate::{
    config::{AssetSet, RenderSettings},
    markers::*,
    materials::MaterialHandleCache,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum MapGeometryKind {
    Ground,
    Roof,
    Wall,
    Ramp,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct BatchKey {
    kind: MapGeometryKind,
    level: u8,
    material_id: String,
}

#[derive(Default)]
struct MeshBatch {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
}

#[derive(Default)]
pub struct MapGeometryBatch {
    batches: BTreeMap<BatchKey, MeshBatch>,
}

impl MapGeometryBatch {
    #[must_use]
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }

    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.batches.values().map(|batch| batch.positions.len() / 3).sum()
    }

    pub(super) fn add_mesh(
        &mut self,
        kind: MapGeometryKind,
        level: u8,
        material_id: impl Into<String>,
        mesh: &Mesh,
        transform: Transform,
    ) {
        let key = BatchKey {
            kind,
            level,
            material_id: material_id.into(),
        };
        self.batches.entry(key).or_default().append(mesh, transform);
    }

    pub fn flush(
        self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        material_cache: &mut MaterialHandleCache,
        asset_server: &AssetServer,
        asset_set: &AssetSet,
        render_settings: &RenderSettings,
        debug_colors: bool,
    ) {
        for (key, batch) in self.batches {
            if batch.positions.is_empty() {
                continue;
            }

            let material = if debug_colors {
                materials.add(random_debug_material())
            } else {
                let material_def = asset_set.material_by_id(&key.material_id);
                material_cache.standard(
                    &key.material_id,
                    material_def,
                    asset_server,
                    materials,
                    render_settings.texture_anisotropy,
                    render_settings.texture_mipmaps_enabled,
                )
            };
            let mut mesh = batch.into_mesh();
            let _ = mesh.generate_tangents();

            spawn_batch(commands, meshes.add(mesh), material, key.kind, key.level);
        }
    }
}

impl MeshBatch {
    fn append(&mut self, mesh: &Mesh, transform: Transform) {
        let positions = float3_attribute(mesh, Mesh::ATTRIBUTE_POSITION);
        let normals = float3_attribute(mesh, Mesh::ATTRIBUTE_NORMAL);
        let uvs = float2_attribute(mesh, Mesh::ATTRIBUTE_UV_0);
        assert_eq!(positions.len(), normals.len(), "map mesh positions/normals must match");
        assert_eq!(positions.len(), uvs.len(), "map mesh positions/uvs must match");

        for ((position, normal), uv) in positions.iter().zip(normals).zip(uvs) {
            let world_position = transform.transform_point(Vec3::from_array(*position));
            let world_normal = transform.rotation * Vec3::from_array(*normal);
            self.positions.push(world_position.to_array());
            self.normals.push(world_normal.normalize_or_zero().to_array());
            self.uvs.push(*uv);
        }
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh
    }
}

fn float3_attribute(mesh: &Mesh, attribute: MeshVertexAttribute) -> &[[f32; 3]] {
    match mesh.attribute(attribute) {
        Some(VertexAttributeValues::Float32x3(values)) => values,
        _ => panic!("map mesh attribute {attribute:?} must be Float32x3"),
    }
}

fn float2_attribute(mesh: &Mesh, attribute: MeshVertexAttribute) -> &[[f32; 2]] {
    match mesh.attribute(attribute) {
        Some(VertexAttributeValues::Float32x2(values)) => values,
        _ => panic!("map mesh attribute {attribute:?} must be Float32x2"),
    }
}

fn spawn_batch(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    kind: MapGeometryKind,
    level: u8,
) {
    let entity = (
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::default(),
        Visibility::Visible,
        MapLevel(level),
    );

    match kind {
        MapGeometryKind::Ground => {
            commands.spawn((entity, GroundMarker));
        }
        MapGeometryKind::Roof => {
            commands.spawn((entity, RoofMarker));
        }
        MapGeometryKind::Wall => {
            commands.spawn((entity, WallMarker));
        }
        MapGeometryKind::Ramp => {
            commands.spawn((entity, RampMarker));
        }
    }
}

fn random_debug_material() -> StandardMaterial {
    let mut rng = rng();
    StandardMaterial {
        base_color: Color::srgb(
            rng.random_range(0.2..1.0),
            rng.random_range(0.2..1.0),
            rng.random_range(0.2..1.0),
        ),
        ..default()
    }
}
