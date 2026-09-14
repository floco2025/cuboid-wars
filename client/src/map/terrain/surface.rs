use std::{collections::BTreeMap, sync::Arc};

use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use common::protocol::{CarrierId, Floor, MapLayout, TERRAIN_MATERIAL};

use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    constants::GRASS_CHUNK_SIZE,
    map::{
        DebugColorMode, DebugColors,
        grass::{ChunkEntry, ChunkKey, ChunkKind, GrassChunkSource, GrassChunks, GrassPatch},
    },
};

#[derive(Component)]
pub struct TerrainMarker;

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
        let min_chunk_x = (x1 / GRASS_CHUNK_SIZE).floor() as i32;
        let max_chunk_x = (x2 / GRASS_CHUNK_SIZE).floor() as i32;
        let min_chunk_z = (z1 / GRASS_CHUNK_SIZE).floor() as i32;
        let max_chunk_z = (z2 / GRASS_CHUNK_SIZE).floor() as i32;
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
            (key.x as f32 + 0.5) * GRASS_CHUNK_SIZE,
            patches[0].y,
            (key.z as f32 + 0.5) * GRASS_CHUNK_SIZE,
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

#[cfg(test)]
#[path = "tests/surface.rs"]
mod tests;
