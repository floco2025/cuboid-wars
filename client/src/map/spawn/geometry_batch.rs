use std::collections::BTreeMap;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{MeshVertexAttribute, VertexAttributeValues},
    prelude::*,
    render::render_resource::PrimitiveTopology,
};
use rand::{RngExt, rng};

use crate::{
    carriers::CarrierEntities,
    config::{AssetSet, ClientSettings},
    map::{DebugColorMode, GroundMarker, MapLevel, RampMarker, RoofMarker, WallMarker},
    materials::MaterialHandleCache,
};
use common::protocol::CarrierId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum MapGeometryKind {
    Ground,
    Roof,
    Wall,
    Ramp,
}

// In normal rendering mode, we group meshes by their resolved material so
// many segments with the same material share a single GPU batch. In debug
// mode the user wants one color per *logical* segment (e.g., a merged wall
// shows as one color regardless of per-face material differences), so each
// segment goes into its own batch identified by `segment_id`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum BatchGroup {
    Material(String),
    Segment(u32),
}

// A batch never spans carriers: its mesh is baked in one carrier's frame
// and hangs under that carrier's entity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct BatchKey {
    carrier: CarrierId,
    kind: MapGeometryKind,
    level: MapLevel,
    group: BatchGroup,
}

#[derive(Default)]
struct MeshBatch {
    material_id: String,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
}

pub struct MapGeometryBatch {
    mode: DebugColorMode,
    next_segment_id: u32,
    current_segment_id: u32,
    batches: BTreeMap<BatchKey, MeshBatch>,
}

impl Default for MapGeometryBatch {
    fn default() -> Self {
        Self::new(DebugColorMode::Off)
    }
}

impl MapGeometryBatch {
    #[must_use]
    pub fn new(mode: DebugColorMode) -> Self {
        Self {
            mode,
            next_segment_id: 0,
            current_segment_id: 0,
            batches: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }

    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.batches.values().map(|batch| batch.positions.len() / 3).sum()
    }

    // Open a fresh logical-segment scope. Subsequent `add_mesh` calls until
    // the next `begin_segment` are grouped together when debug colors is on.
    pub(super) fn begin_segment(&mut self) {
        self.next_segment_id += 1;
        self.current_segment_id = self.next_segment_id;
    }

    pub(super) fn add_mesh(
        &mut self,
        kind: MapGeometryKind,
        carrier: CarrierId,
        level: MapLevel,
        material_id: impl Into<String>,
        mesh: &Mesh,
        transform: Transform,
    ) {
        let material_id = material_id.into();
        let group = match self.mode {
            DebugColorMode::Off | DebugColorMode::ByMaterial => BatchGroup::Material(material_id.clone()),
            DebugColorMode::BySegment => BatchGroup::Segment(self.current_segment_id),
        };
        let key = BatchKey {
            carrier,
            kind,
            level,
            group,
        };
        let batch = self.batches.entry(key).or_default();
        // Remember any one of the contributing materials; in `Off`/`ByMaterial`
        // mode all contributors share the same one (it's part of the key), and
        // in `BySegment` mode the field is unused so an arbitrary
        // representative is fine.
        if batch.material_id.is_empty() {
            batch.material_id = material_id;
        }
        batch.append(mesh, transform);
    }

    pub fn flush(
        self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        material_cache: &mut MaterialHandleCache,
        asset_server: &AssetServer,
        asset_set: &AssetSet,
        client_settings: &ClientSettings,
        carrier_entities: &CarrierEntities,
    ) {
        let mode = self.mode;
        for (key, batch) in self.batches {
            if batch.positions.is_empty() {
                continue;
            }

            let material = match mode {
                DebugColorMode::Off => {
                    let material_def = asset_set.material_by_id(&batch.material_id);
                    material_cache.standard(
                        &batch.material_id,
                        material_def,
                        asset_server,
                        materials,
                        client_settings.rendering.texture_anisotropy,
                        client_settings.rendering.mipmaps,
                    )
                }
                DebugColorMode::ByMaterial => materials.add(material_color_for(&batch.material_id)),
                DebugColorMode::BySegment => materials.add(random_debug_material()),
            };
            let mut mesh = batch.into_mesh();
            let _ = mesh.generate_tangents();

            spawn_batch(
                commands,
                meshes.add(mesh),
                material,
                key.kind,
                carrier_entities.get(key.carrier),
                key.level,
            );
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
            let carrier_position = transform.transform_point(Vec3::from_array(*position));
            let carrier_normal = transform.rotation * Vec3::from_array(*normal);
            self.positions.push(carrier_position.to_array());
            self.normals.push(carrier_normal.normalize_or_zero().to_array());
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
    carrier: Entity,
    level: MapLevel,
) {
    let entity = (
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::default(),
        Visibility::Visible,
        level,
        ChildOf(carrier),
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

fn material_color_for(material_id: &str) -> StandardMaterial {
    // Deterministic hue from the material name so the same material renders
    // the same color across runs and across the whole map.
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    material_id.hash(&mut hasher);
    let digest = hasher.finish();
    let hue = (digest % 360) as f32;
    let saturation = 0.55;
    let lightness = 0.6;
    StandardMaterial {
        base_color: Color::hsl(hue, saturation, lightness),
        ..default()
    }
}
