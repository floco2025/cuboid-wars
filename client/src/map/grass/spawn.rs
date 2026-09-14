use super::streaming::{ChunkEntry, ChunkKey, ChunkKind, GrassChunkSource, GrassChunks};
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    constants::{GRASS_WIND_DIRECTION_DEGREES, GRASS_WIND_SPEED, GRASS_WIND_STRENGTH, TERRAIN_GRASS_CHUNK_SIZE},
    map::{DebugColorMode, DebugColors},
    materials::{GrassMaterial, GrassWindExtension},
};
use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use common::protocol::{CarrierId, Floor, MapLayout, TERRAIN_MATERIAL};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Component)]
pub struct TerrainMarker;

#[derive(Clone, Copy, Debug)]
pub(in crate::map) struct GrassPatch {
    pub(in crate::map) x1: f32,
    pub(in crate::map) x2: f32,
    pub(in crate::map) z1: f32,
    pub(in crate::map) z2: f32,
    pub(in crate::map) y: f32,
    pub(in crate::map) level: u8,
    pub(in crate::map) carrier: CarrierId,
}

impl GrassPatch {
    fn clipped_to_chunk(floor: Floor, chunk_x: i32, chunk_z: i32) -> Option<Self> {
        let (floor_x1, floor_x2, floor_z1, floor_z2) = floor.bounds_xz();
        let chunk_x1 = chunk_x as f32 * TERRAIN_GRASS_CHUNK_SIZE;
        let chunk_z1 = chunk_z as f32 * TERRAIN_GRASS_CHUNK_SIZE;
        let x1 = floor_x1.max(chunk_x1);
        let x2 = floor_x2.min(chunk_x1 + TERRAIN_GRASS_CHUNK_SIZE);
        let z1 = floor_z1.max(chunk_z1);
        let z2 = floor_z2.min(chunk_z1 + TERRAIN_GRASS_CHUNK_SIZE);
        (x2 > x1 && z2 > z1).then_some(Self {
            x1,
            x2,
            z1,
            z2,
            y: floor.y,
            level: floor.level,
            carrier: floor.carrier,
        })
    }

    pub(super) fn area(self) -> f32 {
        (self.x2 - self.x1) * (self.z2 - self.z1)
    }
}

// Terrain cells are floor slabs with authored sides and bottoms. The ordinary
// geometry batch omits their procedural top faces; this system supplies the
// exact compiled floor footprints (including trim) and registers the grass
// chunks over those same footprints for `grass_streaming_system` to build.
pub fn terrain_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    debug_colors: Res<DebugColors>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: ResMut<GrassChunks>,
    carrier_entities: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    existing: Query<Entity, With<TerrainMarker>>,
) {
    if !layout.is_changed() && !debug_colors.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    if debug_colors.0 != DebugColorMode::Off || layout.terrain.is_empty() {
        return;
    }

    let surface_material = chunks.terrain_material();
    let terrain_floors = layout
        .floors
        .iter()
        .zip(layout.floor_materials.iter())
        .filter_map(|(floor, materials)| (materials.top == TERRAIN_MATERIAL).then_some(*floor))
        .collect::<Vec<_>>();
    let mut surface_groups: BTreeMap<(CarrierId, u8), Vec<Floor>> = BTreeMap::new();
    for floor in terrain_floors.iter().copied() {
        surface_groups
            .entry((floor.carrier, floor.level))
            .or_default()
            .push(floor);
    }
    for ((carrier, level), floors) in surface_groups {
        let (min_x, max_x, min_z, max_z) = floors.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY),
            |(min_x, max_x, min_z, max_z), floor| {
                let (x1, x2, z1, z2) = floor.bounds_xz();
                (min_x.min(x1), max_x.max(x2), min_z.min(z1), max_z.max(z2))
            },
        );
        let origin = Vec3::new(f32::midpoint(min_x, max_x), floors[0].y, f32::midpoint(min_z, max_z));
        commands.spawn((
            TerrainMarker,
            storeys.tag(carrier, level, 0),
            ChildOf(carrier_entities.get(carrier)),
            Mesh3d(meshes.add(terrain_surface_mesh(&floors, origin))),
            MeshMaterial3d(surface_material.clone()),
            Transform::from_translation(origin),
            Visibility::Visible,
            NotShadowCaster,
        ));
    }

    let footprint: Arc<[Floor]> = terrain_floors.into();
    let mut patches_by_chunk: BTreeMap<ChunkKey, Vec<GrassPatch>> = BTreeMap::new();
    for floor in footprint.iter().copied() {
        let (x1, x2, z1, z2) = floor.bounds_xz();
        let min_chunk_x = (x1 / TERRAIN_GRASS_CHUNK_SIZE).floor() as i32;
        let max_chunk_x = (x2 / TERRAIN_GRASS_CHUNK_SIZE).floor() as i32;
        let min_chunk_z = (z1 / TERRAIN_GRASS_CHUNK_SIZE).floor() as i32;
        let max_chunk_z = (z2 / TERRAIN_GRASS_CHUNK_SIZE).floor() as i32;
        for chunk_z in min_chunk_z..=max_chunk_z {
            for chunk_x in min_chunk_x..=max_chunk_x {
                let Some(patch) = GrassPatch::clipped_to_chunk(floor, chunk_x, chunk_z) else {
                    continue;
                };
                let key = ChunkKey {
                    kind: ChunkKind::Terrain,
                    carrier: floor.carrier,
                    level: floor.level,
                    x: chunk_x,
                    z: chunk_z,
                };
                patches_by_chunk.entry(key).or_default().push(patch);
            }
        }
    }
    for (key, patches) in patches_by_chunk {
        let origin = Vec3::new(
            (key.x as f32 + 0.5) * TERRAIN_GRASS_CHUNK_SIZE,
            patches[0].y,
            (key.z as f32 + 0.5) * TERRAIN_GRASS_CHUNK_SIZE,
        );
        chunks.register(
            key,
            ChunkEntry {
                patches,
                source: GrassChunkSource::Patches {
                    footprint: footprint.clone(),
                },
                origin,
                carrier: key.carrier,
                parent: Some(carrier_entities.get(key.carrier)),
                level: Some(storeys.tag(key.carrier, key.level, 0)),
            },
        );
    }
}

pub(super) fn grass_material() -> GrassMaterial {
    let wind_direction = Vec2::from_angle(GRASS_WIND_DIRECTION_DEGREES.to_radians());
    GrassMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.95,
            reflectance: 0.1,
            // Both faces draw with the one upward normal: a flipped back-face
            // normal would point down and render half the blades black.
            cull_mode: None,
            ..default()
        },
        extension: GrassWindExtension {
            wind: Vec4::new(
                wind_direction.x,
                wind_direction.y,
                GRASS_WIND_STRENGTH,
                GRASS_WIND_SPEED,
            ),
        },
    }
}

pub(super) fn terrain_surface_mesh(floors: &[Floor], origin: Vec3) -> Mesh {
    let mut positions = Vec::with_capacity(floors.len() * 4);
    let mut normals = Vec::with_capacity(floors.len() * 4);
    let mut uvs = Vec::with_capacity(floors.len() * 4);
    let mut indices = Vec::with_capacity(floors.len() * 6);
    for floor in floors {
        let base = positions.len() as u32;
        let (x1, x2, z1, z2) = floor.bounds_xz();
        for (x, z) in [(x1, z1), (x2, z1), (x1, z2), (x2, z2)] {
            positions.push((Vec3::new(x, floor.y, z) - origin).to_array());
            normals.push(Vec3::Y.to_array());
            // Carrier-local metres keep carried terrain stable. World-carrier
            // coordinates are exactly those used by exterior ground.
            uvs.push([x, z]);
        }
        indices.extend([base, base + 2, base + 1, base + 1, base + 2, base + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}
